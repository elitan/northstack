#[tokio::main]
async fn main() -> anyhow::Result<()> {
    server::build()
        .run(|pool, _config| async move { Ok(server::AppState::new(pool)) })
        .await
}
