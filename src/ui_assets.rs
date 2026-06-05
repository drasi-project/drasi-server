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

/// Returns the admin UI `index.html` as a string, if present.
///
/// Used by the MCP App resource to construct an inlined variant of the SPA
/// shell (with absolute asset URLs) that renders inside a host sandbox.
pub fn index_html() -> Option<String> {
    UiAssets::get("index.html").map(|f| String::from_utf8_lossy(&f.data).into_owned())
}

/// Returns the textual contents of an embedded UI asset (e.g. a CSS file), if
/// present. Used by the MCP App resource to inline the SPA stylesheet so it
/// renders inside a host `srcdoc` sandbox without an external `<link>`.
pub fn asset_text(path: &str) -> Option<String> {
    UiAssets::get(path).map(|f| String::from_utf8_lossy(&f.data).into_owned())
}

/// Placeholder replaced with the absolute Drasi Server origin when the bridge
/// is inlined into the MCP App resource.
const MCP_BRIDGE_BASE_PLACEHOLDER: &str = "__DRASI_BASE__";

/// Client-side bridge inlined into the admin UI when it runs as an MCP App.
///
/// MCP Apps run inside a host-controlled `srcdoc` sandbox iframe on a
/// *different* origin than the Drasi Server. The admin SPA issues root-relative
/// requests (`/api/v1/...`, SSE via `EventSource`, etc.) that would otherwise
/// resolve against the (opaque) sandbox origin. This classic script runs before
/// the app module and rewrites those requests to the absolute Drasi Server
/// origin (injected as `base`). It also applies the saved theme, mirroring the
/// SPA's normal inline bootstrap.
///
/// Note: static `<script src=...>` tags do not execute inside a `srcdoc`
/// iframe, so this bridge is inlined and the app entry is loaded via dynamic
/// `import()` (see the MCP module). Build the runtime form with
/// [`mcp_bridge_js`].
const MCP_BRIDGE_JS: &str = r#"(function () {
  try {
    var base = "__DRASI_BASE__";

    try {
      var t = localStorage.getItem('drasi-theme');
      if (t !== 'light') document.documentElement.classList.add('dark');
    } catch (e) {}

    function rewrite(url) {
      try {
        if (typeof url !== 'string') return url;
        if (url.length > 1 && url.charAt(0) === '/' && url.charAt(1) !== '/') {
          return base + url;
        }
      } catch (e) {}
      return url;
    }

    if (window.fetch) {
      var origFetch = window.fetch.bind(window);
      window.fetch = function (input, init) {
        try {
          if (typeof input === 'string') {
            input = rewrite(input);
          } else if (input && typeof input.url === 'string') {
            var u = rewrite(input.url);
            if (u !== input.url) input = new Request(u, input);
          }
        } catch (e) {}
        return origFetch(input, init);
      };
    }

    if (window.XMLHttpRequest) {
      var origOpen = XMLHttpRequest.prototype.open;
      XMLHttpRequest.prototype.open = function (method, url) {
        try { arguments[1] = rewrite(url); } catch (e) {}
        return origOpen.apply(this, arguments);
      };
    }

    if (window.EventSource) {
      var OrigES = window.EventSource;
      var PatchedES = function (url, config) {
        return new OrigES(rewrite(url), config);
      };
      PatchedES.prototype = OrigES.prototype;
      try {
        PatchedES.CONNECTING = OrigES.CONNECTING;
        PatchedES.OPEN = OrigES.OPEN;
        PatchedES.CLOSED = OrigES.CLOSED;
      } catch (e) {}
      window.EventSource = PatchedES;
    }

    if (window.WebSocket) {
      var OrigWS = window.WebSocket;
      var wsBase = base.replace(/^http/, 'ws');
      var PatchedWS = function (url, protocols) {
        var u = url;
        try {
          if (typeof url === 'string' && url.charAt(0) === '/' && url.charAt(1) !== '/') {
            u = wsBase + url;
          }
        } catch (e) {}
        return protocols === undefined ? new OrigWS(u) : new OrigWS(u, protocols);
      };
      PatchedWS.prototype = OrigWS.prototype;
      try {
        PatchedWS.CONNECTING = OrigWS.CONNECTING;
        PatchedWS.OPEN = OrigWS.OPEN;
        PatchedWS.CLOSING = OrigWS.CLOSING;
        PatchedWS.CLOSED = OrigWS.CLOSED;
      } catch (e) {}
      window.WebSocket = PatchedWS;
    }
  } catch (e) {
    try { console.error('drasi mcp bridge init failed', e); } catch (e2) {}
  }
})();
"#;

/// Build the runtime MCP App bridge script with the Drasi Server origin
/// inlined as `base`.
pub fn mcp_bridge_js(base_url: &str) -> String {
    MCP_BRIDGE_JS.replace(MCP_BRIDGE_BASE_PLACEHOLDER, base_url)
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
