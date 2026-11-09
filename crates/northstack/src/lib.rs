//! # northstack
//!
//! A small, explicit framework for single-binary web apps:
//! Rust (axum + sqlx/Postgres) on the server, React + TypeScript in the
//! browser, with generated types in between.
//!
//! - [`Rpc`]: type-safe procedures, served as `POST /rpc/{name}`
//! - [`codegen`]: generates the TypeScript client + TanStack Query hooks
//! - [`jobs`]: durable Postgres job queue, safe across replicas
//! - [`cron`]: scheduled jobs, safe across replicas
//! - [`cpu`]: thread pool for CPU-heavy work
//! - [`App`]: one binary with `all | serve | worker | migrate | gen-ts` modes

pub mod app;
pub mod codegen;
pub mod cpu;
pub mod cron;
pub mod error;
pub mod jobs;
pub mod rpc;
pub mod schema;
pub mod telemetry;
pub mod testing;
mod web;

pub use app::{App, Config, migrate};
pub use cron::Cron;
pub use error::{ErrorCode, RpcError};
pub use jobs::{EnqueueOptions, Job, Jobs, WorkerConfig, enqueue, enqueue_with};
pub use rpc::{Ctx, Kind, Rpc};

/// Result type for procedures.
pub type RpcResult<T> = Result<T, RpcError>;

// Re-exports so apps depend on one version of each.
pub use axum;
pub use chrono;
pub use rust_embed;
pub use serde;
pub use serde_json;
pub use sqlx;
pub use tokio;
pub use tracing;
pub use ts_rs;
pub use ts_rs::TS;
pub use uuid;
