//! API types for notes. Row types (`Note`, `NotePriority`) are generated from
//! Postgres in `crate::db`; this file holds inputs and query-specific outputs.
//! `#[derive(TS)]` generates the TypeScript definitions.

pub use crate::db::{Note, NotePriority};

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use ts_rs::TS;
use uuid::Uuid;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase", default)]
pub struct ListNotesInput {
    /// Case-insensitive match on title or body.
    #[ts(optional)]
    pub search: Option<String>,
    /// Filter by done state; omit for all.
    #[ts(optional)]
    pub done: Option<bool>,
    /// Max rows, 1..=200. Defaults to 50.
    #[ts(optional)]
    pub limit: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct ListNotesOutput {
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteId {
    pub id: Uuid,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CreateNoteInput {
    pub title: String,
    #[serde(default)]
    #[ts(optional)]
    pub body: Option<String>,
    /// Defaults to `normal`.
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<NotePriority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct UpdateNoteInput {
    pub id: Uuid,
    #[serde(default)]
    #[ts(optional)]
    pub title: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub body: Option<String>,
    #[serde(default)]
    #[ts(optional)]
    pub done: Option<bool>,
    #[serde(default)]
    #[ts(optional)]
    pub priority: Option<NotePriority>,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct DeleteNoteOutput {
    pub deleted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct NoteStats {
    pub total: i64,
    pub done: i64,
    pub words: i64,
    /// The most recent daily digest, if any.
    pub last_digest_day: Option<NaiveDate>,
}
