//! The application. Features live in `features/<name>/` and are wired up in
//! [`build`]. See CLAUDE.md for conventions.

pub mod db;
pub mod features;
pub mod state;

pub use state::AppState;

use northstack::App;
use sqlx::migrate::Migrator;

/// App migrations from `app/migrations`.
pub static MIGRATOR: Migrator = sqlx::migrate!();

/// The frontend, built by Vite into `web/dist` and embedded in release builds.
#[derive(rust_embed::RustEmbed)]
#[folder = "../web/dist"]
struct Assets;

/// Wire up every feature. Add new features here.
pub fn build() -> App<AppState> {
    App::new()
        .migrations(&MIGRATOR)
        .frontend::<Assets>()
        .rpc(features::notes::rpc())
        .jobs(features::notes::jobs())
        .cron(features::notes::cron())
    // northstack:features (new features are registered above this line)
}
