#[tokio::main]
async fn main() -> anyhow::Result<()> {
    app::build()
        .run(|pool, _config| async move { Ok(app::AppState::new(pool)) })
        .await
}
