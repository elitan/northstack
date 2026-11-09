//! RPC handlers. Each is a plain async function:
//! `async fn(Ctx<AppState>, Input) -> RpcResult<Output>`.

use northstack::{Ctx, RpcError, RpcResult, ensure};

use super::db;
use super::jobs::AnalyzeNote;
use super::types::*;
use crate::AppState;

const MAX_TITLE: usize = 200;

pub async fn list(ctx: Ctx<AppState>, input: ListNotesInput) -> RpcResult<ListNotesOutput> {
    let limit = input.limit.unwrap_or(50);
    ensure!(
        (1..=200).contains(&limit),
        "limit must be between 1 and 200"
    );
    let search = input
        .search
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty());
    let notes = db::list(&ctx.state.db, search, input.done, limit).await?;
    Ok(ListNotesOutput { notes })
}

pub async fn get(ctx: Ctx<AppState>, input: NoteId) -> RpcResult<Note> {
    db::get(&ctx.state.db, input.id)
        .await?
        .ok_or_else(|| RpcError::not_found("note not found"))
}

pub async fn create(ctx: Ctx<AppState>, input: CreateNoteInput) -> RpcResult<Note> {
    let title = validate_title(&input.title)?;
    let body = input.body.unwrap_or_default();

    // The note and its analyze job commit together, or not at all.
    let mut tx = ctx.state.db.begin().await?;
    let priority = input.priority.unwrap_or(NotePriority::Normal);
    let note = db::insert(&mut *tx, title, &body, priority).await?;
    northstack::enqueue(&mut *tx, &AnalyzeNote { note_id: note.id }).await?;
    tx.commit().await?;
    Ok(note)
}

pub async fn update(ctx: Ctx<AppState>, input: UpdateNoteInput) -> RpcResult<Note> {
    let title = input.title.as_deref().map(validate_title).transpose()?;
    let body_changed = input.body.is_some();

    let mut tx = ctx.state.db.begin().await?;
    let note = db::update(
        &mut *tx,
        input.id,
        title,
        input.body.as_deref(),
        input.done,
        input.priority,
    )
    .await?
    .ok_or_else(|| RpcError::not_found("note not found"))?;
    if body_changed {
        northstack::enqueue(&mut *tx, &AnalyzeNote { note_id: note.id }).await?;
    }
    tx.commit().await?;
    Ok(note)
}

pub async fn delete(ctx: Ctx<AppState>, input: NoteId) -> RpcResult<DeleteNoteOutput> {
    let deleted = db::delete(&ctx.state.db, input.id).await?;
    Ok(DeleteNoteOutput { deleted })
}

pub async fn stats(ctx: Ctx<AppState>, _input: ()) -> RpcResult<NoteStats> {
    Ok(db::stats(&ctx.state.db).await?)
}

fn validate_title(title: &str) -> RpcResult<&str> {
    let title = title.trim();
    ensure!(!title.is_empty(), "title must not be empty");
    ensure!(
        title.chars().count() <= MAX_TITLE,
        "title must be at most {MAX_TITLE} characters"
    );
    Ok(title)
}
