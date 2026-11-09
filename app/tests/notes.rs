//! End-to-end tests for the notes feature through the real HTTP router.
//! Each test gets a fresh database from `#[sqlx::test]`.

use std::time::Duration;

use app::features::notes::DailyDigest;
use app::features::notes::types::*;
use app::{AppState, MIGRATOR};
use northstack::testing::Client;
use northstack::{ErrorCode, RpcError};
use serde_json::json;
use sqlx::PgPool;

async fn setup(pool: &PgPool) -> Client {
    northstack::migrate(pool, &MIGRATOR).await.unwrap();
    let router = app::build().router(
        AppState::new(pool.clone()),
        pool.clone(),
        Duration::from_secs(30),
    );
    Client::new(router)
}

async fn create(client: &Client, title: &str, body: &str) -> Note {
    client
        .call("notes.create", &json!({ "title": title, "body": body }))
        .await
        .unwrap()
}

#[sqlx::test(migrations = false)]
async fn create_get_list(pool: PgPool) {
    let client = setup(&pool).await;
    let note = create(&client, "  Groceries  ", "milk eggs bread").await;
    assert_eq!(note.title, "Groceries", "title is trimmed");
    assert!(!note.done);
    assert_eq!(
        note.word_count, None,
        "word count is computed in the background"
    );

    let fetched: Note = client
        .call("notes.get", &json!({ "id": note.id }))
        .await
        .unwrap();
    assert_eq!(fetched.id, note.id);

    let list: ListNotesOutput = client.call("notes.list", &json!({})).await.unwrap();
    assert_eq!(list.notes.len(), 1);
}

#[sqlx::test(migrations = false)]
async fn list_filters(pool: PgPool) {
    let client = setup(&pool).await;
    let a = create(&client, "Alpha", "first").await;
    create(&client, "Beta", "second alpha mention").await;
    create(&client, "Gamma", "third").await;
    let _: Note = client
        .call("notes.update", &json!({ "id": a.id, "done": true }))
        .await
        .unwrap();

    let search: ListNotesOutput = client
        .call("notes.list", &json!({ "search": "ALPHA" }))
        .await
        .unwrap();
    assert_eq!(
        search.notes.len(),
        2,
        "search matches title or body, case-insensitive"
    );

    let done: ListNotesOutput = client
        .call("notes.list", &json!({ "done": true }))
        .await
        .unwrap();
    assert_eq!(
        done.notes
            .iter()
            .map(|n| n.title.as_str())
            .collect::<Vec<_>>(),
        ["Alpha"]
    );

    let limited: ListNotesOutput = client
        .call("notes.list", &json!({ "limit": 2 }))
        .await
        .unwrap();
    assert_eq!(limited.notes.len(), 2);
    assert_eq!(limited.notes[0].title, "Gamma", "newest first");
}

#[sqlx::test(migrations = false)]
async fn validation_errors(pool: PgPool) {
    let client = setup(&pool).await;
    let err = client
        .call::<_, Note>("notes.create", &json!({ "title": "   " }))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::BadRequest);
    assert_eq!(err.message, "title must not be empty");

    let long = "x".repeat(201);
    let err = client
        .call::<_, Note>("notes.create", &json!({ "title": long }))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::BadRequest);

    let err = client
        .call::<_, ListNotesOutput>("notes.list", &json!({ "limit": 0 }))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::BadRequest);

    // Wrong input shape is rejected before reaching the handler.
    let err = client
        .call::<_, Note>("notes.create", &json!({ "title": 42 }))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::BadRequest);
    assert!(err.message.starts_with("invalid input"));
}

#[sqlx::test(migrations = false)]
async fn not_found(pool: PgPool) {
    let client = setup(&pool).await;
    let id = uuid::Uuid::new_v4();
    let err: RpcError = client
        .call::<_, Note>("notes.get", &json!({ "id": id }))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = client
        .call::<_, Note>("notes.update", &json!({ "id": id, "done": true }))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
    let err = client
        .call::<_, Note>("notes.nope", &json!({}))
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[sqlx::test(migrations = false)]
async fn update_and_delete(pool: PgPool) {
    let client = setup(&pool).await;
    let note = create(&client, "Draft", "one two").await;
    let updated: Note = client
        .call(
            "notes.update",
            &json!({ "id": note.id, "title": "Final", "done": true }),
        )
        .await
        .unwrap();
    assert_eq!(
        (updated.title.as_str(), updated.done, updated.body.as_str()),
        ("Final", true, "one two")
    );

    let out: DeleteNoteOutput = client
        .call("notes.delete", &json!({ "id": note.id }))
        .await
        .unwrap();
    assert!(out.deleted);
    let out: DeleteNoteOutput = client
        .call("notes.delete", &json!({ "id": note.id }))
        .await
        .unwrap();
    assert!(!out.deleted, "deleting twice is not an error");
}

/// Create enqueues the analyze job in the same transaction; running it fills
/// in the word count. Changing the body resets and recomputes it.
#[sqlx::test(migrations = false)]
async fn background_word_count(pool: PgPool) {
    let client = setup(&pool).await;
    let state = AppState::new(pool.clone());
    let jobs = app::build();
    let jobs = jobs.jobs_ref();

    let note = create(&client, "Essay", "the quick brown fox").await;
    assert_eq!(jobs.run_pending(&pool, state.clone()).await.unwrap(), 1);
    let fetched: Note = client
        .call("notes.get", &json!({ "id": note.id }))
        .await
        .unwrap();
    assert_eq!(fetched.word_count, Some(4));

    // Title-only change: no new job, count kept.
    let _: Note = client
        .call(
            "notes.update",
            &json!({ "id": note.id, "title": "Essay 2" }),
        )
        .await
        .unwrap();
    assert_eq!(jobs.run_pending(&pool, state.clone()).await.unwrap(), 0);

    let changed: Note = client
        .call(
            "notes.update",
            &json!({ "id": note.id, "body": "jumps over the lazy dog today" }),
        )
        .await
        .unwrap();
    assert_eq!(changed.word_count, None);
    assert_eq!(jobs.run_pending(&pool, state.clone()).await.unwrap(), 1);
    let fetched: Note = client
        .call("notes.get", &json!({ "id": note.id }))
        .await
        .unwrap();
    assert_eq!(fetched.word_count, Some(6));

    let stats: NoteStats = client.call("notes.stats", &json!(null)).await.unwrap();
    assert_eq!((stats.total, stats.words), (1, 6));
}

/// The analyze job tolerates a note deleted before it runs.
#[sqlx::test(migrations = false)]
async fn job_for_deleted_note_is_a_noop(pool: PgPool) {
    let client = setup(&pool).await;
    let note = create(&client, "Temp", "a b c").await;
    let _: DeleteNoteOutput = client
        .call("notes.delete", &json!({ "id": note.id }))
        .await
        .unwrap();
    let app = app::build();
    app.jobs_ref()
        .run_pending(&pool, AppState::new(pool.clone()))
        .await
        .unwrap();
    let dead: i64 =
        sqlx::query_scalar("SELECT count(*) FROM northstack_jobs WHERE status <> 'done'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(dead, 0);
}

/// The cron entry enqueues the digest for the previous day, idempotently.
#[sqlx::test(migrations = false)]
async fn daily_digest_cron(pool: PgPool) {
    let client = setup(&pool).await;
    create(&client, "One", "a b").await;
    let app = app::build();
    let state = AppState::new(pool.clone());
    app.jobs_ref()
        .run_pending(&pool, state.clone())
        .await
        .unwrap();

    let at = chrono::Utc::now() + chrono::Duration::days(1);
    assert!(
        app.cron_ref()
            .fire(&pool, "notes.daily_digest", at)
            .await
            .unwrap()
    );
    assert!(
        !app.cron_ref()
            .fire(&pool, "notes.daily_digest", at)
            .await
            .unwrap(),
        "second replica is a no-op"
    );
    app.jobs_ref()
        .run_pending(&pool, state.clone())
        .await
        .unwrap();

    let stats: NoteStats = client.call("notes.stats", &json!(null)).await.unwrap();
    assert_eq!(stats.last_digest_day, Some(chrono::Utc::now().date_naive()));

    // Re-running the same digest job is safe (upsert).
    northstack::enqueue(
        &pool,
        &DailyDigest {
            day: chrono::Utc::now().date_naive(),
        },
    )
    .await
    .unwrap();
    app.jobs_ref().run_pending(&pool, state).await.unwrap();
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM note_digests")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 1);
}

#[sqlx::test(migrations = false)]
async fn health_and_frontend(pool: PgPool) {
    let client = setup(&pool).await;
    assert_eq!(client.get("/healthz").await.0, 200);
    assert_eq!(client.get("/readyz").await.0, 200);
    // Client-side routes fall back to index.html (or a hint if not built).
    let (status, _) = client.get("/some/client/route").await;
    assert!(status == 200 || status == 404);
    // Missing static files are real 404s.
    assert_eq!(client.get("/missing.js").await.0, 404);
}

/// The generated TypeScript client must match the Rust procedures.
#[test]
fn typescript_client_is_up_to_date() {
    let dir = concat!(env!("CARGO_MANIFEST_DIR"), "/../web/src/rpc");
    app::build().rpc_ref().check_ts(dir).unwrap();
}

/// The `note_priority` Postgres enum flows through Rust to the API: invalid
/// values are rejected, and lists sort high to low.
#[sqlx::test(migrations = false)]
async fn priority_enum_from_postgres(pool: PgPool) {
    let client = setup(&pool).await;
    let low: Note = client
        .call(
            "notes.create",
            &json!({ "title": "Later", "priority": "low" }),
        )
        .await
        .unwrap();
    let default: Note = client
        .call("notes.create", &json!({ "title": "Normal" }))
        .await
        .unwrap();
    let high: Note = client
        .call(
            "notes.create",
            &json!({ "title": "Now", "priority": "high" }),
        )
        .await
        .unwrap();
    assert_eq!(low.priority, NotePriority::Low);
    assert_eq!(default.priority, NotePriority::Normal);

    let list: ListNotesOutput = client.call("notes.list", &json!({})).await.unwrap();
    let ids: Vec<_> = list.notes.iter().map(|n| n.id).collect();
    assert_eq!(ids, [high.id, default.id, low.id]);

    let err = client
        .call::<_, Note>(
            "notes.create",
            &json!({ "title": "x", "priority": "urgent" }),
        )
        .await
        .unwrap_err();
    assert_eq!(err.code, ErrorCode::BadRequest);

    let updated: Note = client
        .call("notes.update", &json!({ "id": low.id, "priority": "high" }))
        .await
        .unwrap();
    assert_eq!(updated.priority, NotePriority::High);
}
