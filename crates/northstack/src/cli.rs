//! The `northstack` command line: creating apps, the development loop,
//! database commands and upgrades.
//!
//! An app runs it at the version it depends on: its `cli/` crate is one line,
//! `northstack::cli::main()`, and the `justfile` calls `cargo run -p cli`. The
//! same binary installed globally (`cargo install northstack`) is only needed
//! for `northstack new`.
//!
//! ```text
//! northstack new <dir>              create an app (the example "notes" app)
//! northstack check [--full | --ci]  regenerate, compile, typecheck (+ lint, tests, build)
//! northstack dev [stop|status|reload|logs]
//! northstack e2e [playwright args]  browser smoke tests against the real binary
//! northstack new-feature <name>     scaffold server/src/features/<name>
//! northstack remove-example         delete the example app
//! northstack upgrade [version]      move the app to another northstack version
//! northstack migrate | migration <name> | db-create | db-reset | gen-db
//! ```
//!
//! Inside an app every command runs in the app root (found by walking up from
//! the current directory) with `.env` loaded.

use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, bail, ensure};
use clap::{Parser, Subcommand};
use sqlx::SqlitePool;
use sqlx::migrate::Migrator;

/// This crate's version: the framework version an app runs.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
const REPO: &str = "https://github.com/elitan/northstack";

const CHECK_SH: &str = include_str!("scripts/check.sh");
const DEV_SH: &str = include_str!("scripts/dev.sh");
const E2E_SH: &str = include_str!("scripts/e2e.sh");
const NEW_FEATURE_SH: &str = include_str!("scripts/new-feature.sh");
const REMOVE_EXAMPLE_SH: &str = include_str!("scripts/remove-example.sh");

/// The framework guide, written to `docs/northstack.md` in every app.
pub const GUIDE: &str = include_str!("../GUIDE.md");
const CHANGELOG: &str = include_str!("../CHANGELOG.md");
const APP_AGENTS_MD: &str = include_str!("template/AGENTS.md");
const APP_README_MD: &str = include_str!("template/README.md");
/// Where an app keeps its copy of the guide.
pub const GUIDE_PATH: &str = "docs/northstack.md";

#[derive(Parser)]
#[command(
    name = "northstack",
    version,
    about = "northstack: build web apps with AI agents"
)]
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
    /// Create a new app in <dir>: the example "notes" app, ready for `just dev`.
    New {
        dir: PathBuf,
        /// Use this northstack checkout (template and framework, as a path
        /// dependency) instead of the published version. For framework work.
        #[arg(long, value_name = "CHECKOUT")]
        local: Option<PathBuf>,
    },
    /// Migrate, regenerate, compile, typecheck, and reload the dev api.
    Check {
        /// Also format, lint, test, refresh the query cache and build the frontend.
        #[arg(long)]
        full: bool,
        /// Like --full, but fail on stale generated files instead of writing them.
        #[arg(long, conflicts_with = "full")]
        ci: bool,
        /// Don't restart the running dev api.
        #[arg(long)]
        no_reload: bool,
    },
    /// The dev environment: `dev` (start), `dev stop`, `dev status`, `dev reload`, `dev logs`.
    Dev {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Recent warnings and errors from the dev api and Vite (`logs all`: everything).
    Logs {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Browser smoke tests (web/e2e) against the release-style binary.
    E2e {
        /// Passed to Playwright, e.g. a file filter.
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Scaffold a feature module in server/src/features/<name>.
    NewFeature { name: String },
    /// Delete the example "notes" app and start from an empty one.
    RemoveExample {
        /// Run even with uncommitted changes.
        #[arg(long)]
        force: bool,
    },
    /// Move the app to another northstack version (default: the latest) and
    /// print the upgrade notes for every version in between.
    Upgrade {
        version: Option<String>,
        /// Run even with uncommitted changes.
        #[arg(long)]
        force: bool,
    },
    /// The second half of `upgrade`, run by the new version.
    #[command(hide = true)]
    UpgradeFinish {
        #[arg(long)]
        from: String,
    },
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

/// Entry point for the `northstack` binary and an app's `cli` crate.
pub fn main() {
    // Inside an app, load .env before parsing (DATABASE_URL is read from the
    // environment), as `just` does.
    // A fresh clone has only .env.example.
    if let Some(root) = find_root() {
        let env = root.join(".env");
        if !env.exists() && std::fs::copy(root.join(".env.example"), &env).is_ok() {
            eprintln!("created .env from .env.example");
        }
        load_dotenv(&env);
    }
    let code = match run(Cli::parse()) {
        Ok(code) => code,
        Err(e) => {
            eprintln!("error: {e:#}");
            1
        }
    };
    std::process::exit(code);
}

fn run(cli: Cli) -> anyhow::Result<i32> {
    if let Cmd::New { dir, local } = &cli.cmd {
        new_app(dir, local.as_deref())?;
        return Ok(0);
    }
    let root = app_root()?;
    std::env::set_current_dir(&root)?;
    let url = || {
        cli.database_url.clone().ok_or_else(|| {
            anyhow::anyhow!("DATABASE_URL is not set (run `just dev` once; it writes .env)")
        })
    };
    match &cli.cmd {
        Cmd::New { .. } => unreachable!(),
        Cmd::Check {
            full,
            ci,
            no_reload,
        } => {
            let mut args = vec![];
            if *full {
                args.push("--full");
            }
            if *ci {
                args.push("--ci");
            }
            if *no_reload {
                args.push("--no-reload");
            }
            script("check", CHECK_SH, &args)
        }
        Cmd::Dev { args } => script("dev", DEV_SH, args),
        Cmd::Logs { args } => {
            let args: Vec<&str> = std::iter::once("logs")
                .chain(args.iter().map(String::as_str))
                .collect();
            script("dev", DEV_SH, &args)
        }
        Cmd::E2e { args } => script("e2e", E2E_SH, args),
        Cmd::NewFeature { name } => script("new-feature", NEW_FEATURE_SH, &[name]),
        Cmd::RemoveExample { force } => script(
            "remove-example",
            REMOVE_EXAMPLE_SH,
            if *force { &["--force"] } else { &[] },
        ),
        Cmd::Upgrade { version, force } => upgrade(version.as_deref(), *force),
        Cmd::UpgradeFinish { from } => upgrade_finish(from),
        Cmd::Migration { name } => {
            ensure!(
                name.chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_'),
                "migration name must be snake_case"
            );
            std::fs::create_dir_all(&cli.migrations)?;
            let ts = chrono::Utc::now().format("%Y%m%d%H%M%S");
            let path = cli.migrations.join(format!("{ts}_{name}.sql"));
            std::fs::write(&path, format!("-- {name}\n"))?;
            println!("{}", path.display());
            Ok(0)
        }
        Cmd::DbCreate => block_on(async {
            connect(&url()?).await?.close().await;
            Ok(0)
        }),
        Cmd::DbReset => {
            let url = url()?;
            block_on(db_reset(&url, &cli.migrations))?;
            // The running dev api would keep using the deleted file.
            if Path::new(".dev/active").exists() {
                return script("dev", DEV_SH, &["reload"]);
            }
            Ok(0)
        }
        Cmd::Migrate => block_on(async {
            let pool = connect(&url()?).await?;
            migrate(&pool, &cli.migrations).await?;
            pool.close().await;
            Ok(0)
        }),
        Cmd::GenDb {
            out,
            sqlx_toml,
            module,
            check,
        } => block_on(gen_db(&url()?, out, sqlx_toml, module, *check)),
    }
}

// --- app root, environment, scripts ------------------------------------------

/// The nearest directory at or above the current one that holds an app
/// (`Cargo.toml`, `server/`, `web/`).
fn find_root() -> Option<PathBuf> {
    let cwd = std::env::current_dir().ok()?;
    cwd.ancestors()
        .find(|d| {
            d.join("Cargo.toml").is_file()
                && d.join("server/Cargo.toml").is_file()
                && d.join("web/package.json").is_file()
        })
        .map(Path::to_path_buf)
}

/// The app root, after checking that this binary is the version the app
/// depends on (a global `northstack` may be older or newer).
fn app_root() -> anyhow::Result<PathBuf> {
    let root = find_root().with_context(|| {
        "not inside a northstack app (no Cargo.toml with server/ and web/ here or above); \
         create one with `northstack new <dir>`"
    })?;
    let manifest = std::fs::read_to_string(root.join("Cargo.toml"))?;
    if let Some(Dep::Version(v)) = dep_spec(&manifest)
        && v != VERSION
    {
        bail!(
            "this app uses northstack {v}, but this is northstack {VERSION}. Run commands through \
             `just` (it runs the app's own version) or `cargo run -q -p cli -- <command>`"
        );
    }
    Ok(root)
}

/// Set the variables from a `.env` file that aren't set already.
fn load_dotenv(path: &Path) {
    let Ok(text) = std::fs::read_to_string(path) else {
        return;
    };
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key.trim();
        let value = value.trim().trim_matches(|c| c == '"' || c == '\'');
        if !key.is_empty() && std::env::var_os(key).is_none() {
            // SAFETY: called at startup, before any other thread exists.
            unsafe { std::env::set_var(key, value) };
        }
    }
}

/// Run an embedded bash script in the app root. `$NORTHSTACK` is this binary,
/// so scripts call back into the same version.
fn script<S: AsRef<str>>(name: &str, source: &str, args: &[S]) -> anyhow::Result<i32> {
    let exe = std::env::current_exe().context("locating the northstack binary")?;
    let status = Command::new("bash")
        .arg("-c")
        .arg(source)
        .arg(format!("northstack {name}"))
        .args(args.iter().map(AsRef::as_ref))
        .env("NORTHSTACK", exe)
        .status()
        .context("running bash")?;
    Ok(status.code().unwrap_or(1))
}

fn block_on<T>(f: impl Future<Output = anyhow::Result<T>>) -> anyhow::Result<T> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(f)
}

// --- database commands -------------------------------------------------------

async fn connect(url: &str) -> anyhow::Result<SqlitePool> {
    crate::db::connect(url, 1).await
}

async fn migrate(pool: &SqlitePool, dir: &Path) -> anyhow::Result<()> {
    let app = if dir.exists() {
        Migrator::new(dir.to_path_buf()).await?
    } else {
        Migrator::DEFAULT
    };
    crate::migrate(pool, &app).await.map_err(|e| {
        anyhow::anyhow!("{e:#}\nhint: never edit an applied migration; add a new one. In development, `just db-reset` starts over.")
    })
}

async fn db_reset(url: &str, migrations: &Path) -> anyhow::Result<()> {
    let path = crate::db::file_path(url)
        .map_err(|e| anyhow::anyhow!("db-reset needs a database file: {e}"))?;
    for suffix in ["", "-wal", "-shm"] {
        let mut p = path.clone().into_os_string();
        p.push(suffix);
        match std::fs::remove_file(&p) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => bail!("deleting {}: {e}", p.to_string_lossy()),
        }
    }
    let pool = connect(url).await?;
    migrate(&pool, migrations).await?;
    pool.close().await;
    println!("database reset and migrated: {}", path.display());
    Ok(())
}

async fn gen_db(
    url: &str,
    out: &Path,
    sqlx_toml: &Path,
    module: &str,
    check: bool,
) -> anyhow::Result<i32> {
    let pool = connect(url).await?;
    let schema = crate::schema::Schema::load(&pool).await?;
    pool.close().await;
    let source = crate::schema::rustfmt(&schema.render()?);
    let toml = schema.render_sqlx_toml(module);
    for (path, content) in [(out, source), (sqlx_toml, toml)] {
        let current = std::fs::read_to_string(path).unwrap_or_default();
        if check {
            ensure!(
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
    Ok(0)
}

// --- northstack new ----------------------------------------------------------

/// Files in the northstack repository that belong to the framework, not to
/// apps. Everything else tracked in the repository is the app template.
fn is_framework_file(path: &str) -> bool {
    const DIRS: &[&str] = &["crates/", "bench/", "scripts/", "docs/"];
    const FILES: &[&str] = &[
        "Cargo.toml",
        "README.md",
        "AGENTS.md",
        "LICENSE",
        ".github/workflows/framework.yml",
        ".github/workflows/release.yml",
    ];
    DIRS.iter().any(|d| path.starts_with(d)) || FILES.contains(&path)
}

fn new_app(dir: &Path, local: Option<&Path>) -> anyhow::Result<()> {
    if dir.exists() {
        ensure!(
            dir.read_dir()?.next().is_none(),
            "{} already exists and isn't empty",
            dir.display()
        );
    }
    let name = std::path::absolute(dir)?
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("app")
        .to_string();

    // The template: a local checkout, or the repository at this version's tag.
    let (source, dep, _clone) = match local {
        Some(path) => {
            let path = std::fs::canonicalize(path)
                .with_context(|| format!("--local {}", path.display()))?;
            let crate_dir = path.join("crates/northstack");
            let dep = format!("{{ path = {:?} }}", crate_dir.to_string_lossy());
            (path, dep, None)
        }
        None => {
            let tmp = TempDir::new("northstack-new")?;
            let tag = format!("v{VERSION}");
            println!("downloading the template ({REPO}, {tag})");
            let out = Command::new("git")
                .args(["clone", "--quiet", "--depth", "1", "--branch", &tag, REPO])
                .arg(&tmp.0)
                .output()
                .context("running git (is it installed?)")?;
            ensure!(
                out.status.success(),
                "downloading the template failed: {}",
                String::from_utf8_lossy(&out.stderr).trim()
            );
            (tmp.0.clone(), format!("\"={VERSION}\""), Some(tmp))
        }
    };
    ensure!(
        source.join("crates/northstack/Cargo.toml").is_file() && source.join("server").is_dir(),
        "{} is not a northstack checkout",
        source.display()
    );

    // Tracked files plus new ones that aren't ignored (so a --local checkout
    // with uncommitted changes is copied as it is).
    let out = Command::new("git")
        .args(["ls-files", "-z", "-co", "--exclude-standard"])
        .current_dir(&source)
        .output()
        .context("running git")?;
    ensure!(
        out.status.success(),
        "{} is not a git checkout",
        source.display()
    );
    let files = String::from_utf8(out.stdout)?;
    std::fs::create_dir_all(dir)?;
    for file in files.split('\0').filter(|f| !f.is_empty()) {
        let from = source.join(file);
        if is_framework_file(file) || !from.is_file() {
            continue;
        }
        let to = dir.join(file);
        std::fs::create_dir_all(to.parent().unwrap())?;
        std::fs::copy(&from, &to).with_context(|| format!("copying {file}"))?;
    }

    let manifest = std::fs::read_to_string(source.join("Cargo.toml"))?;
    std::fs::write(dir.join("Cargo.toml"), app_manifest(&manifest, &dep)?)?;
    std::fs::write(dir.join("AGENTS.md"), APP_AGENTS_MD)?;
    std::fs::write(
        dir.join("README.md"),
        APP_README_MD.replace("{{name}}", &name),
    )?;
    write_guide(dir)?;

    let git = |args: &[&str]| {
        Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .is_ok_and(|o| o.status.success())
    };
    let committed = git(&["init", "-q"])
        && git(&["add", "-A"])
        && git(&[
            "commit",
            "-q",
            "-m",
            &format!("Create app with northstack {VERSION}"),
        ]);

    println!("\ncreated {} (northstack {VERSION})", dir.display());
    if !committed {
        println!("(git init or the first commit failed; commit the files yourself)");
    }
    println!(
        "\nnext:\n  cd {}\n  just dev          # http://localhost:5173\n\nThen point your coding agent at it; it reads AGENTS.md.",
        dir.display()
    );
    Ok(())
}

/// The app's workspace manifest, from the repository's: members `server` and
/// `cli`, and northstack from `dep` instead of the repository path.
fn app_manifest(repo_manifest: &str, dep: &str) -> anyhow::Result<String> {
    let manifest = replace_dep(repo_manifest, dep)
        .context("the repository's Cargo.toml has no `northstack` in [workspace.dependencies]")?;
    let mut members = false;
    let mut out = String::new();
    for line in manifest.lines() {
        if line.starts_with("members = ") {
            out.push_str("members = [\"server\", \"cli\"]");
            members = true;
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    ensure!(members, "the repository's Cargo.toml has no `members` line");
    Ok(out)
}

fn write_guide(root: &Path) -> anyhow::Result<()> {
    let path = root.join(GUIDE_PATH);
    std::fs::create_dir_all(path.parent().unwrap())?;
    let header = format!(
        "<!-- The northstack {VERSION} guide. Generated by `northstack new` and \
         `just upgrade`; don't edit, it is replaced on upgrade. -->\n\n"
    );
    std::fs::write(path, header + GUIDE)?;
    Ok(())
}

/// A temporary directory, removed on drop.
struct TempDir(PathBuf);

impl TempDir {
    fn new(prefix: &str) -> anyhow::Result<Self> {
        let path = std::env::temp_dir().join(format!("{prefix}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

// --- northstack upgrade ------------------------------------------------------

/// How an app's workspace depends on northstack.
#[derive(Debug, PartialEq)]
enum Dep {
    Version(String),
    Path,
}

/// Index of the `northstack = ...` line in `[workspace.dependencies]` (not a
/// `[patch]` or other section).
fn dep_line(manifest: &str) -> Option<usize> {
    let mut section = "";
    for (i, line) in manifest.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            section = trimmed;
        } else if section == "[workspace.dependencies]"
            && trimmed
                .strip_prefix("northstack")
                .is_some_and(|rest| rest.trim_start().starts_with('='))
        {
            return Some(i);
        }
    }
    None
}

/// `manifest` with the northstack dependency set to `value`.
fn replace_dep(manifest: &str, value: &str) -> Option<String> {
    let index = dep_line(manifest)?;
    let mut out = String::new();
    for (i, line) in manifest.lines().enumerate() {
        if i == index {
            out.push_str(&format!("northstack = {value}"));
        } else {
            out.push_str(line);
        }
        out.push('\n');
    }
    Some(out)
}

fn dep_spec(manifest: &str) -> Option<Dep> {
    let line = manifest.lines().nth(dep_line(manifest)?)?;
    let value = line.split_once('=')?.1;
    if value.contains("path") {
        return Some(Dep::Path);
    }
    let version = value.split('"').nth(1)?;
    Some(Dep::Version(
        version.trim_start_matches(['=', '^', '~']).to_string(),
    ))
}

/// `1.2.3` as numbers; a pre-release (`1.2.3-rc.1`) sorts before its release.
fn version_key(v: &str) -> (u64, u64, u64, bool) {
    let (core, pre) = match v.split_once('-') {
        Some((core, _)) => (core, true),
        None => (v, false),
    };
    let mut n = core.split('.').map(|p| p.parse().unwrap_or(0));
    (
        n.next().unwrap_or(0),
        n.next().unwrap_or(0),
        n.next().unwrap_or(0),
        !pre,
    )
}

/// The changelog sections for versions after `from`, up to and including `to`
/// (never "Unreleased").
fn upgrade_notes(changelog: &str, from: &str, to: &str) -> Vec<String> {
    let (from, to) = (version_key(from), version_key(to));
    changelog
        .split("\n## ")
        .skip(1)
        .filter(|section| {
            let version = section.split_whitespace().next().unwrap_or("");
            let key = version_key(version);
            version.starts_with(|c: char| c.is_ascii_digit()) && key > from && key <= to
        })
        .map(|section| format!("## {}", section.trim_end()))
        .collect()
}

fn latest_version() -> anyhow::Result<String> {
    let out = Command::new("cargo")
        .args(["search", "northstack", "--limit", "1"])
        .output()
        .context("running cargo search")?;
    ensure!(
        out.status.success(),
        "cargo search failed: {}",
        String::from_utf8_lossy(&out.stderr).trim()
    );
    let stdout = String::from_utf8(out.stdout)?;
    stdout
        .lines()
        .find_map(|l| l.strip_prefix("northstack = \""))
        .and_then(|rest| rest.split('"').next())
        .map(str::to_string)
        .context("northstack was not found on crates.io")
}

fn upgrade(target: Option<&str>, force: bool) -> anyhow::Result<i32> {
    let manifest = std::fs::read_to_string("Cargo.toml")?;
    let current = match dep_spec(&manifest) {
        Some(Dep::Version(v)) => v,
        Some(Dep::Path) => bail!(
            "northstack is a path dependency here (the framework repository, or an app made with \
             `--local`); there is no version to upgrade"
        ),
        None => bail!("Cargo.toml has no `northstack = ...` line in [workspace.dependencies]"),
    };
    let target = match target {
        Some(v) => v.trim_start_matches('v').to_string(),
        None => latest_version()?,
    };
    if target == current {
        println!("already on northstack {current}");
        return Ok(0);
    }
    ensure!(
        version_key(&target) > version_key(&current),
        "{target} is older than the current {current}; downgrades aren't supported"
    );
    if !force {
        let status = Command::new("git").args(["status", "--porcelain"]).output();
        if let Ok(out) = status
            && out.status.success()
            && !out.stdout.is_empty()
        {
            bail!(
                "uncommitted changes; commit them first so the upgrade is its own diff (or pass --force)"
            );
        }
    }

    println!("northstack {current} -> {target}");
    let updated = replace_dep(&manifest, &format!("\"={target}\""))
        .context("Cargo.toml has no `northstack` in [workspace.dependencies]")?;
    std::fs::write("Cargo.toml", updated)?;
    let update = Command::new("cargo")
        .args(["update", "--quiet", "--package", "northstack"])
        .status()?;
    if !update.success() {
        std::fs::write("Cargo.toml", &manifest)?;
        bail!("cargo update failed (is {target} published?); Cargo.toml is unchanged");
    }
    // The rest runs in the new version: it has the new notes, guide and scripts.
    let status = Command::new("cargo")
        .args([
            "run",
            "--quiet",
            "--package",
            "cli",
            "--",
            "upgrade-finish",
            "--from",
            &current,
        ])
        .status()?;
    Ok(status.code().unwrap_or(1))
}

fn upgrade_finish(from: &str) -> anyhow::Result<i32> {
    write_guide(Path::new("."))?;
    let notes = upgrade_notes(CHANGELOG, from, VERSION);
    println!("\nupgraded northstack {from} -> {VERSION}; updated {GUIDE_PATH}\n");
    if notes.is_empty() {
        println!("no upgrade notes between these versions");
    } else {
        println!("upgrade notes (from the northstack changelog):\n");
        for section in &notes {
            println!("{section}\n");
        }
    }
    println!("checking the app against the new version (just check)\n");
    let code = script("check", CHECK_SH, &[] as &[&str])?;
    if code == 0 {
        println!("\nnext: follow any upgrade steps above, then `just verify` and commit");
    } else {
        println!(
            "\nthe app doesn't build against northstack {VERSION} yet: follow the upgrade notes \
             above, fix what `just check` reports, then `just verify` and commit"
        );
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPO_MANIFEST: &str = r#"[workspace]
resolver = "2"
members = ["crates/northstack", "server", "cli"]

[workspace.dependencies]
northstack = { path = "crates/northstack" }

[profile.release]
lto = "thin"
"#;

    #[test]
    fn app_manifest_pins_the_version() {
        let app = app_manifest(REPO_MANIFEST, "\"=0.3.1\"").unwrap();
        assert!(app.contains("members = [\"server\", \"cli\"]\n"));
        assert!(app.contains("\nnorthstack = \"=0.3.1\"\n"));
        assert!(app.contains("lto = \"thin\""));
        assert_eq!(dep_spec(&app), Some(Dep::Version("0.3.1".into())));
        assert_eq!(dep_spec(REPO_MANIFEST), Some(Dep::Path));
        assert!(app_manifest("[workspace]\n", "\"=1\"").is_err());
    }

    #[test]
    fn dep_spec_reads_common_forms() {
        for (line, want) in [
            ("northstack = \"=0.2.0\"", "0.2.0"),
            ("northstack = \"0.2\"", "0.2"),
            ("northstack= { version = \"^0.2.1\" }", "0.2.1"),
        ] {
            let manifest = format!("[workspace.dependencies]\n{line}\n");
            assert_eq!(
                dep_spec(&manifest),
                Some(Dep::Version(want.into())),
                "{line}"
            );
        }
        assert_eq!(
            dep_spec("[workspace.dependencies]\nnorthstack-extra = \"1\"\n"),
            None
        );
        // Only [workspace.dependencies] counts.
        assert_eq!(dep_spec("[patch.crates-io]\nnorthstack = \"1\"\n"), None);
    }

    #[test]
    fn replace_dep_changes_only_the_workspace_dependency() {
        let app = app_manifest(REPO_MANIFEST, "\"=0.1.0\"").unwrap()
            + "\n[patch.crates-io]\nnorthstack = { path = \"../northstack\" }\n";
        let next = replace_dep(&app, "\"=0.2.0\"").unwrap();
        assert_eq!(next, app.replace("\"=0.1.0\"", "\"=0.2.0\""));
        assert!(next.contains("northstack = { path = \"../northstack\" }"));
    }

    #[test]
    fn versions_compare_numerically() {
        assert!(version_key("0.10.0") > version_key("0.9.9"));
        assert!(version_key("1.0.0") > version_key("1.0.0-rc.1"));
        assert!(version_key("0.2.0") == version_key("0.2.0"));
    }

    #[test]
    fn upgrade_notes_cover_skipped_versions() {
        let log = "# Changelog\n\nintro\n\n## Unreleased\n\nnext\n\n## 0.3.0\n\nthree\n\n## 0.2.0\n\ntwo\n\n## 0.1.0\n\none\n";
        assert_eq!(
            upgrade_notes(log, "0.1.0", "0.3.0"),
            ["## 0.3.0\n\nthree", "## 0.2.0\n\ntwo"]
        );
        assert_eq!(upgrade_notes(log, "0.2.0", "0.2.0"), Vec::<String>::new());
        assert_eq!(upgrade_notes(log, "0.0.0", "9.0.0").len(), 3);
        // The real changelog parses; released versions are all numbered.
        assert!(
            upgrade_notes(CHANGELOG, "0.0.0", VERSION)
                .iter()
                .all(|s| !s.starts_with("## Unreleased"))
        );
    }

    #[test]
    fn framework_files_are_not_copied() {
        for f in [
            "crates/northstack/src/lib.rs",
            "bench/bench.py",
            "README.md",
            "Cargo.toml",
            ".github/workflows/framework.yml",
            ".github/workflows/release.yml",
        ] {
            assert!(is_framework_file(f), "{f}");
        }
        for f in [
            "server/src/lib.rs",
            "web/app/page.tsx",
            "cli/src/main.rs",
            "justfile",
            ".github/workflows/ci.yml",
            "Cargo.lock",
            ".sqlx/query-1.json",
        ] {
            assert!(!is_framework_file(f), "{f}");
        }
    }
}
