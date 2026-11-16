//! `northstack`: development commands that don't depend on the app compiling.
//!
//! ```text
//! northstack db-create                 create the database file in DATABASE_URL
//! northstack db-reset                  delete the file, create and migrate
//! northstack migrate                   apply framework + app migrations
//! northstack migration <name>          create server/migrations/<timestamp>_<name>.sql
//! northstack gen-db [--check]          write server/src/db/schema.rs and server/sqlx.toml
//! ```
//!
//! Reads DATABASE_URL from the environment (`just` loads `.env`).

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;

#[derive(Parser)]
#[command(name = "northstack", about = "northstack development commands")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// The SQLite database, e.g. `sqlite:data/dev.db`.
    #[arg(long, env = "DATABASE_URL", global = true)]
    database_url: Option<String>,
    /// App migrations directory.
    #[arg(long, default_value = "server/migrations", global = true)]
    migrations: PathBuf,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create the database file if it doesn't exist.
    DbCreate,
    /// Delete, recreate and migrate the database. Destroys all data.
    DbReset,
    /// Apply framework and app migrations.
    Migrate,
    /// Create a new, empty app migration.
    Migration { name: String },
    /// Generate Rust types (and the sqlx column types) from the database schema.
    GenDb {
        #[arg(long, default_value = "server/src/db/schema.rs")]
        out: PathBuf,
        /// sqlx macro config with the column types, read by `query_as!`.
        #[arg(long, default_value = "server/sqlx.toml")]
        sqlx_toml: PathBuf,
        /// Rust path of the generated module, used in sqlx.toml.
        #[arg(long, default_value = "crate::db")]
        module: String,
        /// Fail if the file is out of date instead of writing it.
        #[arg(long)]
        check: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(e) = run(Cli::parse()).await {
        eprintln!("error: {e:#}");
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> anyhow::Result<()> {
    let url = || {
        cli.database_url.clone().ok_or_else(|| {
            anyhow::anyhow!("DATABASE_URL is not set (run `just dev` once; it writes .env)")
        })
    };
    match &cli.cmd {
        Cmd::Migration { name } => {
            anyhow::ensure!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "migration name must be snake_case"
            );
            std::fs::create_dir_all(&cli.migrations)?;
            let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
            let path = cli.migrations.join(format!("{ts}_{name}.sql"));
            std::fs::write(&path, format!("-- {name}\n"))?;
            println!("{}", path.display());
        }
        Cmd::DbCreate => {
            connect(&url()?).await?.close().await;
        }
        Cmd::DbReset => {
            let url = url()?;
            let path = northstack::db::file_path(&url)
                .map_err(|e| anyhow::anyhow!("db-reset needs a database file: {e}"))?;
            for suffix in ["", "-wal", "-shm"] {
                let mut p = path.clone().into_os_string();
                p.push(suffix);
                match std::fs::remove_file(&p) {
                    Ok(()) => {}
                    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                    Err(e) => anyhow::bail!("deleting {}: {e}", p.to_string_lossy()),
                }
            }
            let pool = connect(&url).await?;
            migrate(&pool, &cli.migrations).await?;
            pool.close().await;
            println!("database reset and migrated: {}", path.display());
        }
        Cmd::Migrate => {
            let pool = connect(&url()?).await?;
            migrate(&pool, &cli.migrations).await?;
            pool.close().await;
        }
        Cmd::GenDb {
            out,
            sqlx_toml,
            module,
            check,
        } => {
            let pool = connect(&url()?).await?;
            let schema = northstack::schema::Schema::load(&pool).await?;
            pool.close().await;
            let source = northstack::schema::rustfmt(&schema.render()?);
            let toml = schema.render_sqlx_toml(module);
            for (path, content) in [(out, source), (sqlx_toml, toml)] {
                let current = std::fs::read_to_string(path).unwrap_or_default();
                if *check {
                    anyhow::ensure!(
                        current == content,
                        "{} is out of date with the database schema; run `just check`",
                        path.display()
                    );
                } else if current != content {
                    if let Some(dir) = path.parent().filter(|d| !d.as_os_str().is_empty()) {
                        std::fs::create_dir_all(dir)?;
                    }
                    std::fs::write(path, content)?;
                    println!("updated {}", path.display());
                }
            }
        }
    }
    Ok(())
}

async fn connect(url: &str) -> anyhow::Result<SqlitePool> {
    northstack::db::connect(url, 1).await
}

async fn migrate(pool: &SqlitePool, dir: &std::path::Path) -> anyhow::Result<()> {
    let app = if dir.exists() {
        Migrator::new(dir.to_path_buf()).await?
    } else {
        Migrator::DEFAULT
    };
    northstack::migrate(pool, &app).await.map_err(|e| {
        anyhow::anyhow!("{e:#}\nhint: never edit an applied migration; add a new one. In development, `just db-reset` starts over.")
    })
}
