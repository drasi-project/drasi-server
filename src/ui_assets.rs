// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Embedded UI assets for self-contained binary distribution.
//!
//! In release builds, UI files from `ui/dist/` are embedded into the binary
//! at compile time. In debug builds, `rust-embed` reads from the filesystem
//! automatically, enabling hot-reload during development.

use axum::body::Body;
use axum::extract::Path;
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use rust_embed::Embed;

#[derive(Embed)]
#[folder = "ui/dist"]
struct UiAssets;

/// Returns true if the embedded UI contains actual assets (i.e., the UI was
/// built before the Rust binary was compiled).
pub fn has_embedded_ui() -> bool {
    UiAssets::get("index.html").is_some()
}

/// Creates axum routes that serve the embedded UI assets under `/ui`.
pub fn embedded_ui_routes() -> Router {
    Router::new()
        .route("/ui", get(serve_index))
        .route("/ui/", get(serve_index))
        .route("/ui/*path", get(serve_path))
}

async fn serve_index() -> Response {
    serve_asset("index.html")
}

async fn serve_path(Path(path): Path<String>) -> Response {
    serve_asset(&path)
}

fn has_file_extension(path: &str) -> bool {
    std::path::Path::new(path)
        .extension()
        .is_some_and(|ext| !ext.is_empty())
}

fn serve_asset(path: &str) -> Response {
    // Try the exact path first
    let (content, served_path) = if let Some(file) = UiAssets::get(path) {
        (file, path)
    } else if has_file_extension(path) {
        // Paths with file extensions (e.g. .js, .css) are static assets —
        // return 404 so broken builds surface clearly instead of silently
        // serving index.html with the wrong content-type.
        return StatusCode::NOT_FOUND.into_response();
    } else if let Some(file) = UiAssets::get("index.html") {
        // Route-like paths without extensions get the SPA fallback
        (file, "index.html")
    } else {
        return StatusCode::NOT_FOUND.into_response();
    };

    let mime = mime_guess::from_path(served_path)
        .first_or_octet_stream()
        .to_string();
    Response::builder()
        .status(StatusCode::OK)
        .header(header::CONTENT_TYPE, mime)
        .body(Body::from(content.data.to_vec()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}
