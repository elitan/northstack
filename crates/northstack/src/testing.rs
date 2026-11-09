//! Helpers for integration tests.
//!
//! Use with `#[sqlx::test(migrations = false)]`, which gives every test a
//! fresh, isolated database:
//!
//! ```ignore
//! #[sqlx::test(migrations = false)]
//! async fn creates_note(pool: PgPool) {
//!     northstack::migrate(&pool, &app::MIGRATOR).await.unwrap();
//!     let client = northstack::testing::Client::new(app::build().router_for_test(state, pool));
//!     ...
//! }
//! ```

use axum::body::Body;
use axum::http::{Request, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;
use tower::ServiceExt;

use crate::RpcError;

/// In-process HTTP client for an axum router. No network involved.
#[derive(Clone)]
pub struct Client {
    router: axum::Router,
    headers: Vec<(String, String)>,
}

impl Client {
    pub fn new(router: axum::Router) -> Self {
        Self {
            router,
            headers: Vec::new(),
        }
    }

    /// Add a header to every request (e.g. authorization).
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        self.headers.push((name.to_string(), value.to_string()));
        self
    }

    /// Call a procedure and decode the typed result.
    pub async fn call<I: Serialize, O: DeserializeOwned>(
        &self,
        name: &str,
        input: &I,
    ) -> Result<O, RpcError> {
        let (status, body) = self
            .post_raw(&format!("/rpc/{name}"), serde_json::to_vec(input).unwrap())
            .await;
        if status.is_success() {
            Ok(serde_json::from_slice(&body).unwrap_or_else(|e| {
                panic!(
                    "decoding output of {name}: {e}; body: {}",
                    String::from_utf8_lossy(&body)
                )
            }))
        } else {
            Err(serde_json::from_slice(&body).unwrap_or_else(|e| {
                panic!(
                    "decoding error of {name} ({status}): {e}; body: {}",
                    String::from_utf8_lossy(&body)
                )
            }))
        }
    }

    /// POST raw bytes; returns status and body.
    pub async fn post_raw(&self, path: &str, body: Vec<u8>) -> (StatusCode, Vec<u8>) {
        let mut req = Request::post(path).header("content-type", "application/json");
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        self.send(req.body(Body::from(body)).unwrap()).await
    }

    /// GET a path; returns status and body.
    pub async fn get(&self, path: &str) -> (StatusCode, Vec<u8>) {
        let mut req = Request::get(path);
        for (k, v) in &self.headers {
            req = req.header(k, v);
        }
        self.send(req.body(Body::empty()).unwrap()).await
    }

    async fn send(&self, req: Request<Body>) -> (StatusCode, Vec<u8>) {
        let res = self
            .router
            .clone()
            .oneshot(req)
            .await
            .expect("router is infallible");
        let status = res.status();
        let body = axum::body::to_bytes(res.into_body(), usize::MAX)
            .await
            .unwrap();
        (status, body.to_vec())
    }
}
