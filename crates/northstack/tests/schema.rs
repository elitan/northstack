//! `northstack gen-db`: Rust types generated from a real Postgres schema.

use northstack::schema::Schema;
use sqlx::PgPool;

async fn exec(pool: &PgPool, sql: &'static str) {
    sqlx::raw_sql(sql).execute(pool).await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn generates_rows_enums_and_conventions(pool: PgPool) {
    exec(
        &pool,
        r#"
        CREATE TYPE order_status AS ENUM ('pending', 'paid', 'in-transit');
        CREATE TABLE categories (id INT PRIMARY KEY, name TEXT NOT NULL);
        CREATE TABLE people (
            id UUID PRIMARY KEY,
            email TEXT NOT NULL,
            password_hash TEXT NOT NULL,
            type TEXT,
            tags TEXT[] NOT NULL DEFAULT '{}',
            balance NUMERIC(12, 2),
            status order_status NOT NULL,
            history order_status[],
            settings JSONB,
            born DATE,
            created_at TIMESTAMPTZ NOT NULL DEFAULT now()
        );
        COMMENT ON TABLE people IS 'Someone with an account. @name(Person)';
        COMMENT ON COLUMN people.email IS 'Login email, unique.';
        COMMENT ON COLUMN people.password_hash IS 'Argon2 hash. @private';
        CREATE TABLE _internal (x INT);
        CREATE TABLE audit_log (x INT);
        COMMENT ON TABLE audit_log IS 'Not exposed. @skip';
        "#,
    )
    .await;

    let src = Schema::load(&pool, "public")
        .await
        .unwrap()
        .render()
        .unwrap();

    // Enums: Postgres labels kept exactly, variants in PascalCase.
    assert!(src.contains("#[sqlx(type_name = \"order_status\")]\npub enum OrderStatus {"));
    assert!(src.contains(
        "#[serde(rename = \"in-transit\")]\n    #[sqlx(rename = \"in-transit\")]\n    InTransit,"
    ));
    // Naming: singular by default, overridable.
    assert!(src.contains("pub struct Category {"));
    assert!(src.contains("pub struct Person {"));
    assert!(!src.contains("pub struct People"));
    // Comments become docs; tags are stripped.
    assert!(src.contains("/// Someone with an account.\n"));
    assert!(src.contains("/// Login email, unique.\n    pub email: String,"));
    // @private: kept in Rust, hidden from JSON and TypeScript.
    assert!(src.contains(
        "#[serde(skip_serializing, default)]\n    #[ts(skip)]\n    pub password_hash: String,"
    ));
    // Types and nullability.
    assert!(src.contains("pub r#type: Option<String>,"));
    assert!(src.contains("pub tags: Vec<String>,"));
    assert!(src.contains("pub balance: Option<sqlx::types::BigDecimal>,"));
    assert!(src.contains("pub status: OrderStatus,"));
    assert!(src.contains("pub history: Option<Vec<OrderStatus>>,"));
    assert!(src.contains("pub settings: Option<serde_json::Value>,"));
    assert!(src.contains("pub born: Option<chrono::NaiveDate>,"));
    assert!(src.contains("pub created_at: chrono::DateTime<chrono::Utc>,"));
    // Skipped tables.
    assert!(!src.contains("Internal") && !src.contains("AuditLog"));
}

#[sqlx::test(migrations = false)]
async fn reports_every_unsupported_column(pool: PgPool) {
    exec(&pool, "CREATE TABLE things (a INET, b POINT, c TEXT);").await;
    let err = Schema::load(&pool, "public")
        .await
        .unwrap()
        .render()
        .unwrap_err()
        .to_string();
    assert!(
        err.contains("things.a: Postgres type `inet` is not supported"),
        "{err}"
    );
    assert!(
        err.contains("things.b: Postgres type `point` is not supported"),
        "{err}"
    );
}

#[sqlx::test(migrations = false)]
async fn reports_name_collisions(pool: PgPool) {
    exec(
        &pool,
        "CREATE TABLE note (id INT); CREATE TABLE notes (id INT);",
    )
    .await;
    let err = Schema::load(&pool, "public")
        .await
        .unwrap()
        .render()
        .unwrap_err()
        .to_string();
    assert!(err.contains("would both be named `Note`"), "{err}");
}
