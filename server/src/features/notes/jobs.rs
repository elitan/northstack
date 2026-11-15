//! Background jobs for notes. Handlers must be idempotent: a job can run more
//! than once if a worker crashes mid-way.

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::db;
use crate::AppState;

/// Count the words in a note. Enqueued on create and when the body changes.
#[derive(Debug, Serialize, Deserialize)]
pub struct AnalyzeNote {
    pub note_id: Uuid,
}

impl northstack::Job for AnalyzeNote {
    const KIND: &'static str = "notes.analyze";
}

pub async fn analyze_note(state: AppState, job: AnalyzeNote) -> anyhow::Result<()> {
    // The note may have been deleted since; that's fine.
    let Some(note) = db::get(&state.db, job.note_id).await? else {
        return Ok(());
    };
    // CPU-bound work goes to the CPU pool, never directly on the async runtime.
    let words = northstack::cpu::spawn(move || note.body.split_whitespace().count()).await?;
    db::set_word_count(&state.db, job.note_id, words.try_into().unwrap_or(i32::MAX)).await?;
    Ok(())
}

/// Write the digest row for one day. Scheduled daily by cron.
#[derive(Debug, Serialize, Deserialize)]
pub struct DailyDigest {
    pub day: NaiveDate,
}

impl northstack::Job for DailyDigest {
    const KIND: &'static str = "notes.daily_digest";
    const MAX_ATTEMPTS: i32 = 3;
}

pub async fn daily_digest(state: AppState, job: DailyDigest) -> anyhow::Result<()> {
    db::write_digest(&state.db, job.day).await?;
    tracing::info!(day = %job.day, "wrote daily digest");
    Ok(())
}
