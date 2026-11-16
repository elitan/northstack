use sqlx::SqlitePool;

/// Shared application state, available to every procedure (`ctx.state`) and
/// job handler. Keep it cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
}

impl AppState {
    pub fn new(db: SqlitePool) -> Self {
        Self { db }
    }
}
