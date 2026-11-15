//! All SQL for notes. Queries are checked against the real schema at compile
//! time by `sqlx::query!`/`query_as!`.

use sqlx::{PgExecutor, PgPool};
use uuid::Uuid;

use super::types::NoteStats;
use crate::db::{Note, NotePriority};

pub async fn list(
    db: &PgPool,
    search: Option<&str>,
    done: Option<bool>,
    limit: i64,
) -> sqlx::Result<Vec<Note>> {
    // Optional filters: a NULL parameter disables its condition.
    sqlx::query_as!(
        Note,
        r#"
        SELECT id, title, body, done, word_count, created_at, updated_at,
               priority AS "priority: NotePriority"
        FROM notes
        WHERE ($1::text IS NULL OR title ILIKE '%' || $1 || '%' OR body ILIKE '%' || $1 || '%')
          AND ($2::bool IS NULL OR done = $2)
        ORDER BY priority DESC, created_at DESC
        LIMIT $3
        "#,
        search,
        done,
        limit,
    )
    .fetch_all(db)
    .await
}

pub async fn get<'e>(db: impl PgExecutor<'e>, id: Uuid) -> sqlx::Result<Option<Note>> {
    sqlx::query_as!(
        Note,
        r#"
        SELECT id, title, body, done, word_count, created_at, updated_at,
               priority AS "priority: NotePriority"
        FROM notes WHERE id = $1
        "#,
        id
    )
    .fetch_optional(db)
    .await
}

pub async fn insert<'e>(
    db: impl PgExecutor<'e>,
    title: &str,
    body: &str,
    priority: NotePriority,
) -> sqlx::Result<Note> {
    sqlx::query_as!(
        Note,
        r#"
        INSERT INTO notes (title, body, priority) VALUES ($1, $2, $3)
        RETURNING id, title, body, done, word_count, created_at, updated_at,
                  priority AS "priority: NotePriority"
        "#,
        title,
        body,
        priority as NotePriority
    )
    .fetch_one(db)
    .await
}

/// Partial update: `None` leaves a column unchanged. The word count is reset
/// when the body changes, and recomputed by the analyze job.
pub async fn update<'e>(
    db: impl PgExecutor<'e>,
    id: Uuid,
    title: Option<&str>,
    body: Option<&str>,
    done: Option<bool>,
    priority: Option<NotePriority>,
) -> sqlx::Result<Option<Note>> {
    sqlx::query_as!(
        Note,
        r#"
        UPDATE notes SET
            title      = COALESCE($2, title),
            body       = COALESCE($3, body),
            done       = COALESCE($4, done),
            priority   = COALESCE($5, priority),
            word_count = CASE WHEN $3::text IS NULL THEN word_count ELSE NULL END,
            updated_at = now()
        WHERE id = $1
        RETURNING id, title, body, done, word_count, created_at, updated_at,
                  priority AS "priority: NotePriority"
        "#,
        id,
        title,
        body,
        done,
        priority as Option<NotePriority>
    )
    .fetch_optional(db)
    .await
}

pub async fn delete(db: &PgPool, id: Uuid) -> sqlx::Result<bool> {
    let res = sqlx::query!("DELETE FROM notes WHERE id = $1", id)
        .execute(db)
        .await?;
    Ok(res.rows_affected() > 0)
}

pub async fn set_word_count(db: &PgPool, id: Uuid, words: i32) -> sqlx::Result<()> {
    sqlx::query!("UPDATE notes SET word_count = $2 WHERE id = $1", id, words)
        .execute(db)
        .await?;
    Ok(())
}

pub async fn stats(db: &PgPool) -> sqlx::Result<NoteStats> {
    let row = sqlx::query!(
        r#"
        SELECT
            count(*)                                   AS "total!",
            count(*) FILTER (WHERE done)               AS "done!",
            COALESCE(sum(word_count), 0)::bigint       AS "words!",
            (SELECT max(day) FROM note_digests)        AS last_digest_day
        FROM notes
        "#
    )
    .fetch_one(db)
    .await?;
    Ok(NoteStats {
        total: row.total,
        done: row.done,
        words: row.words,
        last_digest_day: row.last_digest_day,
    })
}

/// Upsert so the digest job is idempotent: re-running it rewrites the row.
pub async fn write_digest(db: &PgPool, day: chrono::NaiveDate) -> sqlx::Result<()> {
    sqlx::query!(
        r#"
        INSERT INTO note_digests (day, total, done, words)
        SELECT $1, count(*), count(*) FILTER (WHERE done), COALESCE(sum(word_count), 0)
        FROM notes WHERE created_at < ($1::date + 1)
        ON CONFLICT (day) DO UPDATE
            SET total = EXCLUDED.total, done = EXCLUDED.done, words = EXCLUDED.words,
                created_at = now()
        "#,
        day
    )
    .execute(db)
    .await?;
    Ok(())
}
