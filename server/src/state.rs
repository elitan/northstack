use sqlx::PgPool;

/// Shared application state, available to every procedure (`ctx.state`) and
/// job handler. Keep it cheap to clone.
#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
}

impl AppState {
    pub fn new(db: PgPool) -> Self {
        Self { db }
    }
}
