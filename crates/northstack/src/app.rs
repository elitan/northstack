//! The application runner: one binary, several modes.
//!
//! ```text
//! app            # same as `app all`
//! app all        # HTTP server + workers + cron in one process (default)
//! app serve      # HTTP server only
//! app worker     # workers + cron only
//! app migrate    # run migrations and exit
//! app gen-ts     # write the TypeScript client (no database needed)
//! ```
//!
//! Configuration is read from the environment, see [`Config`].

use std::future::Future;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::http::{HeaderName, StatusCode};
use axum::response::IntoResponse;
use clap::{Parser, Subcommand};
use rust_embed::RustEmbed;
use sqlx::PgPool;
use sqlx::migrate::Migrator;
use sqlx::postgres::PgPoolOptions;
use tokio_util::sync::CancellationToken;
use tower_http::request_id::{MakeRequestUuid, PropagateRequestIdLayer, SetRequestIdLayer};
use tower_http::trace::TraceLayer;

use crate::cron::Cron;
use crate::jobs::{Jobs, WorkerConfig, run_workers};
use crate::rpc::Rpc;

static NORTHSTACK_MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Run the framework's migrations and then the app's. Both sets live in the
/// same `_sqlx_migrations` table; each ignores the other's entries.
pub async fn migrate(pool: &PgPool, app: &Migrator) -> anyhow::Result<()> {
    let mut northstack = clone_migrator(&NORTHSTACK_MIGRATOR);
    northstack.set_ignore_missing(true);
    northstack.run(pool).await?;
    let mut app = clone_migrator(app);
    app.set_ignore_missing(true);
    app.run(pool).await?;
    Ok(())
}

fn clone_migrator(m: &Migrator) -> Migrator {
    Migrator::with_migrations(m.iter().cloned().collect())
}

#[derive(Parser, Debug)]
#[command(version, about)]
struct Cli {
    #[command(subcommand)]
    mode: Option<Mode>,
}

#[derive(Subcommand, Debug, Clone)]
enum Mode {
    /// HTTP server + workers + cron (default).
    All,
    /// HTTP server only.
    Serve,
    /// Workers and cron only.
    Worker,
    /// Run database migrations and exit.
    Migrate,
    /// Generate the TypeScript client and exit.
    GenTs {
        /// Output directory. Defaults to the app's configured directory.
        #[arg(long)]
        out: Option<PathBuf>,
        /// Fail if the generated files differ instead of writing them.
        #[arg(long)]
        check: bool,
    },
}

/// Runtime configuration from environment variables.
#[derive(Debug, Clone, Parser)]
pub struct Config {
    /// Postgres connection string.
    #[arg(long, env = "DATABASE_URL")]
    pub database_url: String,
    #[arg(long, env = "HOST", default_value = "0.0.0.0")]
    pub host: String,
    #[arg(long, env = "PORT", default_value_t = 3000)]
    pub port: u16,
    /// Max connections in this process's pool.
    #[arg(long, env = "DB_MAX_CONNECTIONS", default_value_t = 10)]
    pub db_max_connections: u32,
    /// Max concurrent jobs in this process.
    #[arg(long, env = "WORKER_CONCURRENCY", default_value_t = 8)]
    pub worker_concurrency: usize,
    /// Use LISTEN/NOTIFY to wake workers. Set false behind PgBouncer
    /// transaction pooling.
    #[arg(long, env = "JOBS_LISTEN", default_value_t = true, action = clap::ArgAction::Set)]
    pub jobs_listen: bool,
    /// Per-request timeout in seconds.
    #[arg(long, env = "REQUEST_TIMEOUT_SECS", default_value_t = 30)]
    pub request_timeout_secs: u64,
    /// Run migrations at startup (in `all`, `serve` and `worker` modes).
    #[arg(long, env = "MIGRATE_ON_START", default_value_t = true, action = clap::ArgAction::Set)]
    pub migrate_on_start: bool,
}

type AssetFn =
    fn(axum::http::Uri) -> std::pin::Pin<Box<dyn Future<Output = axum::response::Response> + Send>>;

/// Builder for the whole application.
pub struct App<S> {
    rpc: Rpc<S>,
    jobs: Jobs<S>,
    cron: Cron,
    migrator: Option<&'static Migrator>,
    assets: Option<AssetFn>,
    routes: Option<axum::Router<S>>,
    ts_out: PathBuf,
}

impl<S: Clone + Send + Sync + 'static> Default for App<S> {
    fn default() -> Self {
        Self {
            rpc: Rpc::new(),
            jobs: Jobs::new(),
            cron: Cron::new(),
            migrator: None,
            assets: None,
            routes: None,
            ts_out: PathBuf::from("web/src/rpc"),
        }
    }
}

impl<S: Clone + Send + Sync + 'static> App<S> {
    pub fn new() -> Self {
        Self::default()
    }
    pub fn rpc(mut self, rpc: Rpc<S>) -> Self {
        self.rpc = self.rpc.merge(rpc);
        self
    }
    pub fn jobs(mut self, jobs: Jobs<S>) -> Self {
        self.jobs = self.jobs.merge(jobs);
        self
    }
    pub fn cron(mut self, cron: Cron) -> Self {
        self.cron = self.cron.merge(cron);
        self
    }
    /// App migrations, usually `&MIGRATOR` where
    /// `static MIGRATOR: Migrator = sqlx::migrate!();`.
    pub fn migrations(mut self, migrator: &'static Migrator) -> Self {
        self.migrator = Some(migrator);
        self
    }
    /// Serve a frontend embedded with `#[derive(RustEmbed)]`.
    pub fn frontend<E: RustEmbed + 'static>(mut self) -> Self {
        self.assets = Some(|uri| Box::pin(crate::web::serve::<E>(uri)));
        self
    }
    /// Extra hand-written axum routes (webhooks, file downloads, ...).
    pub fn routes(mut self, router: axum::Router<S>) -> Self {
        self.routes = Some(match self.routes.take() {
            Some(r) => r.merge(router),
            None => router,
        });
        self
    }
    /// Where `gen-ts` writes the TypeScript client (relative to the working
    /// directory). Default: `web/src/rpc`.
    pub fn ts_out(mut self, dir: impl Into<PathBuf>) -> Self {
        self.ts_out = dir.into();
        self
    }

    pub fn rpc_ref(&self) -> &Rpc<S> {
        &self.rpc
    }
    pub fn jobs_ref(&self) -> &Jobs<S> {
        &self.jobs
    }
    pub fn cron_ref(&self) -> &Cron {
        &self.cron
    }

    /// Build the HTTP router. Exposed for tests.
    pub fn router(&self, state: S, pool: PgPool, request_timeout: Duration) -> axum::Router {
        let mut router = self.rpc.clone().into_router(state.clone());
        if let Some(extra) = self.routes.clone() {
            router = router.merge(extra.with_state(state));
        }
        router = router
            .route("/healthz", axum::routing::get(|| async { "ok" }))
            .route(
                "/readyz",
                axum::routing::get(move || {
                    let pool = pool.clone();
                    async move {
                        match sqlx::query("SELECT 1").execute(&pool).await {
                            Ok(_) => (StatusCode::OK, "ready").into_response(),
                            Err(e) => {
                                tracing::warn!(error = %e, "readiness check failed");
                                (StatusCode::SERVICE_UNAVAILABLE, "database unavailable")
                                    .into_response()
                            }
                        }
                    }
                }),
            );
        if let Some(assets) = self.assets {
            router = router.fallback(move |uri: axum::http::Uri| assets(uri));
        }
        let request_id = HeaderName::from_static("x-request-id");
        router
            .layer(tower_http::timeout::TimeoutLayer::with_status_code(
                StatusCode::REQUEST_TIMEOUT,
                request_timeout,
            ))
            .layer(tower_http::compression::CompressionLayer::new())
            .layer(PropagateRequestIdLayer::new(request_id.clone()))
            .layer(TraceLayer::new_for_http().make_span_with(move |req: &axum::http::Request<_>| {
                let id = req
                    .headers()
                    .get("x-request-id")
                    .and_then(|v| v.to_str().ok())
                    .unwrap_or("-");
                tracing::info_span!("http", method = %req.method(), path = %req.uri().path(), request_id = %id)
            }))
            .layer(SetRequestIdLayer::new(request_id, MakeRequestUuid))
    }

    /// Parse the command line and run. `make_state` builds the app state from
    /// the connected pool.
    pub async fn run<F, Fut>(self, make_state: F) -> anyhow::Result<()>
    where
        F: FnOnce(PgPool, Config) -> Fut,
        Fut: Future<Output = anyhow::Result<S>>,
    {
        let cli = Cli::parse();
        let mode = cli.mode.unwrap_or(Mode::All);

        if let Mode::GenTs { out, check } = &mode {
            let dir = out.clone().unwrap_or_else(|| self.ts_out.clone());
            if *check {
                self.rpc.check_ts(&dir)?;
                println!("TypeScript client in {} is up to date", dir.display());
            } else {
                self.rpc.generate_ts(&dir)?;
                println!("wrote TypeScript client to {}", dir.display());
            }
            return Ok(());
        }

        crate::telemetry::init();
        let config = Config::try_parse_from(["app"])
            .map_err(|e| anyhow::anyhow!("invalid configuration: {e}"))?;
        let pool = PgPoolOptions::new()
            .max_connections(config.db_max_connections)
            .acquire_timeout(Duration::from_secs(5))
            .connect(&config.database_url)
            .await
            .map_err(|e| anyhow::anyhow!("connecting to database: {e}"))?;

        let empty = Migrator::DEFAULT;
        let app_migrator = self.migrator.unwrap_or(&empty);
        if matches!(mode, Mode::Migrate) {
            migrate(&pool, app_migrator).await?;
            tracing::info!("migrations applied");
            return Ok(());
        }
        if config.migrate_on_start {
            migrate(&pool, app_migrator).await?;
        }

        let state = make_state(pool.clone(), config.clone()).await?;
        let shutdown = CancellationToken::new();
        tokio::spawn(wait_for_signal(shutdown.clone()));

        let run_http = matches!(mode, Mode::All | Mode::Serve);
        let run_workers_ = matches!(mode, Mode::All | Mode::Worker);

        let mut tasks = tokio::task::JoinSet::new();
        if run_workers_ {
            let jobs = Arc::new(self.jobs.clone());
            let wc = WorkerConfig {
                concurrency: config.worker_concurrency,
                listen: config.jobs_listen,
                ..WorkerConfig::default()
            };
            tasks.spawn(run_workers(
                pool.clone(),
                jobs,
                state.clone(),
                wc,
                shutdown.clone(),
            ));
        }
        if run_workers_ {
            let cron = self.cron.clone();
            tasks.spawn(cron.run(pool.clone(), shutdown.clone()));
        }
        if run_http {
            let router = self.router(
                state.clone(),
                pool.clone(),
                Duration::from_secs(config.request_timeout_secs),
            );
            let addr: SocketAddr = format!("{}:{}", config.host, config.port).parse()?;
            let listener = tokio::net::TcpListener::bind(addr).await?;
            tracing::info!(%addr, procedures = self.rpc.names().len(), "http listening");
            let token = shutdown.clone();
            tasks.spawn(async move {
                if let Err(e) = axum::serve(listener, router)
                    .with_graceful_shutdown(token.cancelled_owned())
                    .await
                {
                    tracing::error!(error = %e, "http server error");
                }
            });
        }

        while tasks.join_next().await.is_some() {}
        pool.close().await;
        tracing::info!("shut down cleanly");
        Ok(())
    }
}

async fn wait_for_signal(token: CancellationToken) {
    let ctrl_c = tokio::signal::ctrl_c();
    #[cfg(unix)]
    {
        let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("install SIGTERM handler");
        tokio::select! {
            _ = ctrl_c => {}
            _ = term.recv() => {}
        }
    }
    #[cfg(not(unix))]
    let _ = ctrl_c.await;
    tracing::info!("shutdown signal received");
    token.cancel();
}
