//! Serves the frontend embedded in the binary with `rust-embed`.
//!
//! - Files under `assets/` (hashed by Vite) are cached for a year.
//! - Any other path without a matching file returns `index.html`, so client
//!   side routing works.

use axum::http::{StatusCode, Uri, header};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

pub(crate) async fn serve<E: RustEmbed>(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("rpc/") {
        return StatusCode::NOT_FOUND.into_response();
    }
    if !path.is_empty() {
        if let Some(file) = E::get(path) {
            let mime = mime_guess::from_path(path).first_or_octet_stream();
            let cache = if path.starts_with("assets/") {
                "public, max-age=31536000, immutable"
            } else {
                "no-cache"
            };
            return (
                [
                    (header::CONTENT_TYPE, mime.as_ref().to_string()),
                    (header::CACHE_CONTROL, cache.to_string()),
                ],
                file.data.into_owned(),
            )
                .into_response();
        }
        // A missing file with an extension is a real 404, not a client route.
        if path
            .rsplit('/')
            .next()
            .is_some_and(|last| last.contains('.'))
        {
            return StatusCode::NOT_FOUND.into_response();
        }
    }
    match E::get("index.html") {
        Some(index) => (
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-cache"),
            ],
            index.data.into_owned(),
        )
            .into_response(),
        None => (
            StatusCode::NOT_FOUND,
            "frontend not built: run `just web` (or `just build` for a release binary)",
        )
            .into_response(),
    }
}
