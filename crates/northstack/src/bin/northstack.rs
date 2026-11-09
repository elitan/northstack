//! `northstack`: development commands that don't depend on the app compiling.
//!
//! ```text
//! northstack db-create                 create the database in DATABASE_URL
//! northstack db-reset                  drop, create and migrate
//! northstack migrate                   apply framework + app migrations
//! northstack migration <name>          create app/migrations/<timestamp>_<name>.sql
//! northstack gen-db [--check]          write app/src/db/schema.rs from the database
//! ```
//!
//! Reads DATABASE_URL from the environment (`just` loads `.env`).

use std::path::PathBuf;

use clap::{Parser, Subcommand};
use sqlx::migrate::{MigrateDatabase, Migrator};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Postgres};

#[derive(Parser)]
#[command(name = "northstack", about = "northstack development commands")]
struct Cli {
    #[command(subcommand)]
    cmd: Cmd,
    /// Postgres connection string.
    #[arg(long, env = "DATABASE_URL", global = true)]
    database_url: Option<String>,
    /// App migrations directory.
    #[arg(long, default_value = "app/migrations", global = true)]
    migrations: PathBuf,
}

#[derive(Subcommand)]
enum Cmd {
    /// Create the database if it doesn't exist.
    DbCreate,
    /// Drop, recreate and migrate the database. Destroys all data.
    DbReset,
    /// Apply framework and app migrations.
    Migrate,
    /// Create a new, empty app migration.
    Migration { name: String },
    /// Generate Rust types from the database schema.
    GenDb {
        #[arg(long, default_value = "app/src/db/schema.rs")]
        out: PathBuf,
        #[arg(long, default_value = "public")]
        schema: String,
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
        cli.database_url
            .clone()
            .ok_or_else(|| anyhow::anyhow!("DATABASE_URL is not set (copy .env.example to .env)"))
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
            let url = url()?;
            if !Postgres::database_exists(&url).await? {
                Postgres::create_database(&url).await?;
                println!("created database");
            }
        }
        Cmd::DbReset => {
            let url = url()?;
            if Postgres::database_exists(&url).await? {
                Postgres::force_drop_database(&url).await?;
            }
            Postgres::create_database(&url).await?;
            migrate(&connect(&url).await?, &cli.migrations).await?;
            println!("database reset and migrated");
        }
        Cmd::Migrate => {
            let url = url()?;
            if !Postgres::database_exists(&url).await? {
                Postgres::create_database(&url).await?;
            }
            migrate(&connect(&url).await?, &cli.migrations).await?;
        }
        Cmd::GenDb { out, schema, check } => {
            let pool = connect(&url()?).await?;
            let source = northstack::schema::Schema::load(&pool, schema)
                .await?
                .render()?;
            let source = northstack::schema::rustfmt(&source);
            let current = std::fs::read_to_string(out).unwrap_or_default();
            if *check {
                anyhow::ensure!(
                    current == source,
                    "{} is out of date with the database schema; run `just check`",
                    out.display()
                );
            } else if current != source {
                if let Some(dir) = out.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                std::fs::write(out, source)?;
                println!("updated {}", out.display());
            }
        }
    }
    Ok(())
}

async fn connect(url: &str) -> anyhow::Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(url)
        .await
        .map_err(|e| anyhow::anyhow!("connecting to database: {e}"))
}

async fn migrate(pool: &PgPool, dir: &std::path::Path) -> anyhow::Result<()> {
    let app = if dir.exists() {
        Migrator::new(dir.to_path_buf()).await?
    } else {
        Migrator::DEFAULT
    };
    northstack::migrate(pool, &app).await.map_err(|e| {
        anyhow::anyhow!("{e:#}\nhint: never edit an applied migration; add a new one. In development, `just db-reset` starts over.")
    })
}
