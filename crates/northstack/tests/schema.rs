//! `northstack gen-db`: Rust types generated from a real SQLite schema.

use northstack::schema::Schema;
use northstack::testing::{TestDb, db};
use sqlx::migrate::Migrator;

static NONE: Migrator = Migrator::DEFAULT;

async fn with(sql: &'static str) -> TestDb {
    let db = db(&NONE).await;
    sqlx::raw_sql(sql).execute(&*db).await.unwrap();
    db
}

async fn render(db: &TestDb) -> Result<String, String> {
    Schema::load(db)
        .await
        .unwrap()
        .render()
        .map_err(|e| e.to_string())
}

#[tokio::test]
async fn generates_rows_enums_and_conventions() {
    let db = with(
        r#"
        CREATE TABLE categories (id INTEGER PRIMARY KEY, name TEXT NOT NULL);
        CREATE TABLE people ( -- Someone with an account. @name(Person)
            id            UUID    PRIMARY KEY NOT NULL,
            -- Login email, unique.
            email         TEXT    NOT NULL UNIQUE,
            password_hash TEXT    NOT NULL, -- Argon2 hash. @private
            type          TEXT,
            score         REAL,
            visits        INTEGER NOT NULL DEFAULT 0,
            admin         BOOLEAN NOT NULL DEFAULT false,
            status        TEXT    NOT NULL DEFAULT 'paid'
                          CHECK (status IN ('pending', 'paid', 'in-transit')),
            -- Shown on the profile.
            tier          TEXT, -- @enum(Tier)
            settings      JSON,
            avatar        BLOB,
            born          DATE,
            created_at    TIMESTAMP NOT NULL DEFAULT (strftime('%Y-%m-%dT%H:%M:%f+00:00')),
            CHECK (tier IN ('free', 'pro'))
        );
        ALTER TABLE people ADD COLUMN nickname TEXT;
        CREATE TABLE _internal (x INTEGER);
        CREATE TABLE audit_log ( -- Not exposed. @skip
            x INTEGER
        );
        "#,
    )
    .await;
    let src = render(&db).await.unwrap();

    // Enums from CHECK (col IN (...)): values kept exactly, PascalCase variants,
    // the SQL default is the Rust default.
    assert!(src.contains("/// Values of `people.status`."), "{src}");
    assert!(src.contains("pub enum PersonStatus {"), "{src}");
    assert!(src.contains(
        "    #[serde(rename = \"in-transit\")]\n    #[sqlx(rename = \"in-transit\")]\n    InTransit,"
    ));
    assert!(
        src.contains("    #[default]\n    #[serde(rename = \"paid\")]"),
        "{src}"
    );
    // Table-level CHECK and @enum(Name).
    assert!(src.contains("pub enum Tier {"), "{src}");
    assert!(src.contains("pub tier: Option<Tier>,"), "{src}");
    // Naming: singular by default, overridable.
    assert!(src.contains("pub struct Category {"));
    assert!(src.contains("pub struct Person {"));
    assert!(!src.contains("pub struct People"));
    // Comments become docs; tags are stripped. Above and after a column both work.
    assert!(src.contains("/// Someone with an account.\n"), "{src}");
    assert!(
        src.contains("/// Login email, unique.\n    pub email: String,"),
        "{src}"
    );
    assert!(
        src.contains("/// Shown on the profile.\n    pub tier"),
        "{src}"
    );
    // @private: kept in Rust, hidden from JSON and TypeScript.
    assert!(src.contains(
        "/// Argon2 hash.\n    #[serde(skip_serializing, default)]\n    #[ts(skip)]\n    pub password_hash: String,"
    ), "{src}");
    // Types and nullability.
    assert!(
        src.contains("pub id: i64,"),
        "INTEGER PRIMARY KEY is never null: {src}"
    );
    assert!(src.contains("pub id: uuid::Uuid,"));
    assert!(src.contains("pub r#type: Option<String>,"));
    assert!(src.contains("pub score: Option<f64>,"));
    assert!(src.contains("pub visits: i64,"));
    assert!(src.contains("pub admin: bool,"));
    assert!(src.contains("pub status: PersonStatus,"));
    assert!(src.contains("pub settings: Option<serde_json::Value>,"));
    assert!(src.contains("pub avatar: Option<Vec<u8>>,"));
    assert!(src.contains("pub born: Option<chrono::NaiveDate>,"));
    assert!(src.contains("pub created_at: chrono::DateTime<chrono::Utc>,"));
    // Columns added with ALTER TABLE are included.
    assert!(src.contains("pub nickname: Option<String>,"), "{src}");
    // Skipped tables.
    assert!(!src.contains("Internal") && !src.contains("AuditLog"));
    assert!(!src.contains("Northstack"));
}

#[tokio::test]
async fn sqlx_toml_lists_columns_the_macros_cant_infer() {
    let db = with(
        r#"
        CREATE TABLE notes (
            id UUID PRIMARY KEY NOT NULL,
            title TEXT NOT NULL,
            done BOOLEAN NOT NULL,
            priority TEXT NOT NULL CHECK (priority IN ('low', 'high')),
            meta JSON,
            created_at TIMESTAMP NOT NULL
        );
        CREATE TABLE tags (name TEXT NOT NULL);
        "#,
    )
    .await;
    let toml = Schema::load(&db)
        .await
        .unwrap()
        .render_sqlx_toml("crate::db");
    let expected = "[macros.table-overrides.'notes']
'id' = \"uuid::Uuid\"
'priority' = \"crate::db::NotePriority\"
'meta' = \"serde_json::Value\"
'created_at' = \"chrono::DateTime<chrono::Utc>\"
";
    assert!(toml.contains(expected), "{toml}");
    assert!(!toml.contains("'tags'"), "{toml}");
    assert!(
        !toml.contains("'title'") && !toml.contains("'done'"),
        "{toml}"
    );
}

#[tokio::test]
async fn reports_every_unsupported_column() {
    let db = with("CREATE TABLE things (a NUMERIC, b, c POINT, d TEXT);").await;
    let err = render(&db).await.unwrap_err();
    assert!(
        err.contains("things.a: type `NUMERIC` is not supported; SQLite has no exact decimals"),
        "{err}"
    );
    assert!(
        err.contains("things.b: type `` is not supported; give the column a type"),
        "{err}"
    );
    assert!(
        err.contains("things.c: type `POINT` is not supported"),
        "{err}"
    );
    assert!(!err.contains("things.d"), "{err}");
}

#[tokio::test]
async fn rejects_timestamp_defaults_that_sort_wrong() {
    let db = with("CREATE TABLE t (at TIMESTAMP NOT NULL DEFAULT CURRENT_TIMESTAMP);").await;
    let err = render(&db).await.unwrap_err();
    assert!(err.contains("t.at: a TIMESTAMP default must be"), "{err}");
}

#[tokio::test]
async fn reports_name_collisions() {
    let db = with("CREATE TABLE note (id INTEGER); CREATE TABLE notes (id INTEGER);").await;
    let err = render(&db).await.unwrap_err();
    assert!(err.contains("would both be named `Note`"), "{err}");
}

/// Full-text search: the FTS5 table and its shadow tables (`_data`, `_idx`,
/// ...) have untyped columns; they are skipped instead of failing gen-db.
#[tokio::test]
async fn skips_virtual_tables_and_their_shadow_tables() {
    let db = with(
        "CREATE TABLE notes (id INTEGER PRIMARY KEY, body TEXT NOT NULL);
         CREATE VIRTUAL TABLE notes_fts USING fts5(body, content='notes', content_rowid='id');",
    )
    .await;
    let src = render(&db).await.unwrap();
    assert!(src.contains("pub struct Note {"), "{src}");
    assert!(!src.contains("NotesFts"), "{src}");
}

#[tokio::test]
async fn empty_schema_has_no_unused_imports() {
    // An app without tables (e.g. after `just remove-example`) must compile
    // without warnings, so the generated file has no imports to leave unused.
    let db = db(&NONE).await;
    let src = render(&db).await.unwrap();
    assert!(!src.contains("use serde"), "unexpected imports:\n{src}");
    assert!(!src.contains("use ts_rs"), "unexpected imports:\n{src}");
}
