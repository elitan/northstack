//! Type-safe RPC: register plain async functions as procedures and call them
//! from TypeScript with generated types.
//!
//! ```ignore
//! let rpc = northstack::Rpc::<AppState>::new()
//!     .query("notes.list", notes::list)
//!     .mutation("notes.create", notes::create);
//! ```
//!
//! Every procedure is served as `POST /rpc/{name}` with a JSON body (the input)
//! and returns JSON (the output) or an [`RpcError`].

use std::collections::BTreeMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, State};
use axum::http::request::Parts;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde::de::DeserializeOwned;
use ts_rs::TS;

use crate::codegen::TsInfo;
use crate::error::RpcError;

/// Request context passed to every procedure.
pub struct Ctx<S> {
    /// Application state (database pool, config, ...).
    pub state: S,
    /// The HTTP request head: headers, extensions set by middleware (e.g. the
    /// authenticated user), URI, etc.
    pub parts: Parts,
}

impl<S> Ctx<S> {
    /// Fetch a value inserted into request extensions by middleware.
    pub fn ext<T: Clone + Send + Sync + 'static>(&self) -> Option<T> {
        self.parts.extensions.get::<T>().cloned()
    }

    /// Like [`Ctx::ext`] but returns `UNAUTHORIZED` when missing. Use this for
    /// the authenticated principal.
    pub fn require<T: Clone + Send + Sync + 'static>(&self) -> Result<T, RpcError> {
        self.ext::<T>()
            .ok_or_else(|| RpcError::unauthorized("not authenticated"))
    }

    /// Header value as a string, if present and valid UTF-8.
    pub fn header(&self, name: &str) -> Option<&str> {
        self.parts.headers.get(name).and_then(|v| v.to_str().ok())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Query,
    Mutation,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Query => "query",
            Kind::Mutation => "mutation",
        }
    }
}

type BoxFuture<T> = Pin<Box<dyn Future<Output = T> + Send>>;
type ErasedHandler<S> =
    Arc<dyn Fn(Ctx<S>, Bytes) -> BoxFuture<Result<Vec<u8>, RpcError>> + Send + Sync>;

#[derive(Clone)]
pub(crate) struct Procedure<S> {
    pub(crate) kind: Kind,
    handler: ErasedHandler<S>,
    pub(crate) ts: TsInfo,
}

/// A collection of procedures. Build one per feature and [`Rpc::merge`] them.
#[derive(Clone)]
pub struct Rpc<S> {
    pub(crate) procedures: BTreeMap<&'static str, Procedure<S>>,
}

impl<S> Default for Rpc<S> {
    fn default() -> Self {
        Self {
            procedures: BTreeMap::new(),
        }
    }
}

impl<S: Clone + Send + Sync + 'static> Rpc<S> {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a read-only procedure.
    pub fn query<I, O, F, Fut>(self, name: &'static str, f: F) -> Self
    where
        I: DeserializeOwned + TS + Send + 'static,
        O: Serialize + TS + 'static,
        F: Fn(Ctx<S>, I) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<O, RpcError>> + Send + 'static,
    {
        self.add(Kind::Query, name, f)
    }

    /// Register a procedure that changes state.
    pub fn mutation<I, O, F, Fut>(self, name: &'static str, f: F) -> Self
    where
        I: DeserializeOwned + TS + Send + 'static,
        O: Serialize + TS + 'static,
        F: Fn(Ctx<S>, I) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<O, RpcError>> + Send + 'static,
    {
        self.add(Kind::Mutation, name, f)
    }

    fn add<I, O, F, Fut>(mut self, kind: Kind, name: &'static str, f: F) -> Self
    where
        I: DeserializeOwned + TS + Send + 'static,
        O: Serialize + TS + 'static,
        F: Fn(Ctx<S>, I) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = Result<O, RpcError>> + Send + 'static,
    {
        assert!(
            valid_name(name),
            "invalid procedure name {name:?}: use dot-separated camelCase segments, e.g. \"notes.list\""
        );
        let handler: ErasedHandler<S> = Arc::new(move |ctx: Ctx<S>, body: Bytes| {
            let f = f.clone();
            Box::pin(async move {
                let body: &[u8] = if body.is_empty() { b"null" } else { &body };
                let input: I = serde_json::from_slice(body)
                    .map_err(|e| RpcError::bad_request(format!("invalid input: {e}")))?;
                let output = f(ctx, input).await?;
                serde_json::to_vec(&output).map_err(RpcError::internal)
            })
        });
        let previous = self.procedures.insert(
            name,
            Procedure {
                kind,
                handler,
                ts: TsInfo::new::<I, O>(),
            },
        );
        assert!(previous.is_none(), "procedure {name:?} registered twice");
        self
    }

    /// Combine procedures from another feature. Panics on duplicate names.
    pub fn merge(mut self, other: Rpc<S>) -> Self {
        for (name, proc_) in other.procedures {
            let previous = self.procedures.insert(name, proc_);
            assert!(previous.is_none(), "procedure {name:?} registered twice");
        }
        self
    }

    /// Names of all registered procedures, sorted.
    pub fn names(&self) -> Vec<&'static str> {
        self.procedures.keys().copied().collect()
    }

    /// Build the axum router serving `POST /rpc/{name}`.
    pub fn into_router(self, state: S) -> axum::Router {
        let shared = Arc::new(self.procedures);
        axum::Router::new()
            .route("/rpc/{name}", axum::routing::post(dispatch::<S>))
            .with_state((shared, state))
    }

    /// Call a procedure in-process with a JSON value. Intended for tests.
    pub async fn call_json(
        &self,
        state: S,
        name: &str,
        input: serde_json::Value,
    ) -> Result<serde_json::Value, RpcError> {
        let proc_ = self
            .procedures
            .get(name)
            .ok_or_else(|| RpcError::not_found(format!("unknown procedure {name}")))?;
        let (parts, _) = axum::http::Request::new(()).into_parts();
        let body = Bytes::from(serde_json::to_vec(&input).map_err(RpcError::internal)?);
        let out = (proc_.handler)(Ctx { state, parts }, body).await?;
        serde_json::from_slice(&out).map_err(RpcError::internal)
    }
}

type Shared<S> = (Arc<BTreeMap<&'static str, Procedure<S>>>, S);

async fn dispatch<S: Clone + Send + Sync + 'static>(
    State((procedures, state)): State<Shared<S>>,
    Path(name): Path<String>,
    request: axum::extract::Request,
) -> Response {
    let Some(proc_) = procedures.get(name.as_str()) else {
        return RpcError::not_found(format!("unknown procedure {name}")).into_response();
    };
    let (parts, body) = request.into_parts();
    let body = match axum::body::to_bytes(body, 10 * 1024 * 1024).await {
        Ok(b) => b,
        Err(e) => return RpcError::bad_request(format!("invalid body: {e}")).into_response(),
    };
    let span = tracing::info_span!("rpc", procedure = %name, kind = proc_.kind.as_str());
    let result =
        tracing::Instrument::instrument((proc_.handler)(Ctx { state, parts }, body), span).await;
    match result {
        Ok(json) => (
            [(axum::http::header::CONTENT_TYPE, "application/json")],
            json,
        )
            .into_response(),
        Err(err) => {
            if err.code == crate::ErrorCode::Internal {
                tracing::error!(procedure = %name, "procedure failed");
            }
            err.into_response()
        }
    }
}

fn valid_name(name: &str) -> bool {
    !name.is_empty()
        && name.split('.').all(|seg| {
            let mut chars = seg.chars();
            matches!(chars.next(), Some(c) if c.is_ascii_lowercase())
                && chars.all(|c| c.is_ascii_alphanumeric())
        })
}

#[cfg(test)]
mod tests {
    use super::valid_name;

    #[test]
    fn names() {
        assert!(valid_name("notes.list"));
        assert!(valid_name("notes.createDraft"));
        assert!(!valid_name("Notes.list"));
        assert!(!valid_name("notes..list"));
        assert!(!valid_name("notes-list"));
        assert!(!valid_name(""));
    }
}
