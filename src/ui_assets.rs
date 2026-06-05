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

/// Path at which the MCP App bridge script is served.
pub const MCP_BRIDGE_PATH: &str = "/__mcp/bridge.js";

/// Client-side bridge injected into the admin UI when it runs as an MCP App.
///
/// MCP Apps run inside a host-controlled sandbox iframe on a *different* origin
/// than the Drasi Server. This classic (non-module, non-deferred) script runs
/// before the deferred app module. It derives the **asset origin** from its own
/// `src` (the explicit `127.0.0.1` origin the host loaded it from) and computes
/// the **API origin** as the `localhost` variant — because the host normalizes
/// `connect-src` to `localhost`, so the app's API/SSE calls must target
/// `localhost` to pass CSP. It rewrites the SPA's root-relative requests
/// (`/api/v1/...`, SSE via `EventSource`, WebSocket) to that API origin, and
/// applies the saved theme (mirroring the SPA's inline bootstrap, which is
/// stripped).
pub const MCP_BRIDGE_JS: &str = r#"(function () {
  try {
    var me = document.currentScript;
    var assetBase = me ? new URL(me.src).origin : window.location.origin;
    // The host normalizes connect-src to localhost, so address the API there.
    var base = assetBase.replace('127.0.0.1', 'localhost');

    try {
      var t = localStorage.getItem('drasi-theme');
      if (t !== 'light') document.documentElement.classList.add('dark');
    } catch (e) {}

    // --- MCP App host handshake + sizing ---------------------------------
    // MCP Apps run inside a host-controlled sandbox iframe. Per the MCP Apps
    // spec, the host will NOT size the iframe (nor send the View any messages)
    // until the View completes the `ui/initialize` handshake and sends an
    // `initialized` notification. For a *flexible* / *unbounded* height
    // container the View is responsible for telling the host its desired
    // height via `ui/notifications/size-changed` — the host then resizes the
    // iframe to match. The drasi admin UI is a fixed-viewport (100vh / h-screen)
    // SPA, whose intrinsic content height is therefore circular (== iframe
    // height); without this handshake the host leaves the iframe at a tiny
    // default (~150px) and the UI appears blank. We pick a concrete target
    // height (the host's maxHeight when flexible, else a sane default) and
    // report it; 100vh content then fills the resized iframe.
    (function () {
      try {
        if (window.parent === window) return; // not embedded
        var hostWin = window.parent;
        var INIT_ID = 'drasi-ui-init';
        var sizeMode = null; // 'fixed' | 'flexible' | 'unbounded'
        var maxH = null;
        var reportedH = 0;

        function notify(method, params) {
          try {
            hostWin.postMessage(
              { jsonrpc: '2.0', method: method, params: params || {} },
              '*'
            );
          } catch (e) {}
        }

        function applyContainer(cd) {
          if (cd && typeof cd.height === 'number') {
            sizeMode = 'fixed';
            try { document.documentElement.style.height = '100vh'; } catch (e) {}
          } else if (cd && typeof cd.maxHeight === 'number') {
            sizeMode = 'flexible';
            maxH = cd.maxHeight;
          } else {
            sizeMode = 'unbounded';
          }
        }

        function reportSize() {
          if (sizeMode === 'fixed') return; // host controls the height
          var h = maxH ? maxH : 900;
          if (h && h !== reportedH) {
            reportedH = h;
            // Give the SPA a concrete height so its 100vh content fills the
            // iframe once the host grows it to match.
            try { document.documentElement.style.height = h + 'px'; } catch (e) {}
            try { if (document.body) document.body.style.minHeight = h + 'px'; } catch (e) {}
            notify('ui/notifications/size-changed', { height: h });
          }
        }

        function applyHostContext(ctx) {
          if (!ctx) return;
          try {
            applyContainer(ctx.containerDimensions);
            if (ctx.theme === 'dark') document.documentElement.classList.add('dark');
            else if (ctx.theme === 'light') document.documentElement.classList.remove('dark');
          } catch (e) {}
        }

        var handshakeDone = false;
        function finishHandshake(ctx) {
          if (handshakeDone) { applyHostContext(ctx); reportSize(); return; }
          handshakeDone = true;
          applyHostContext(ctx);
          notify('ui/notifications/initialized', {});
          reportSize();
          // Re-report after layout/React have settled.
          setTimeout(reportSize, 300);
          setTimeout(reportSize, 1500);
          setTimeout(reportSize, 4000);
        }

        window.addEventListener('message', function (ev) {
          var d = ev.data;
          if (!d) return;
          if (d.id === INIT_ID && (d.result || d.error)) {
            finishHandshake(d.result ? d.result.hostContext : null);
          } else if (d.method === 'ui/notifications/host-context-changed') {
            // Params carry changed host-context fields (theme, dimensions...).
            applyHostContext(d.params && d.params.hostContext ? d.params.hostContext : d.params);
            reportSize();
          }
        });

        try {
          hostWin.postMessage(
            {
              jsonrpc: '2.0',
              id: INIT_ID,
              method: 'ui/initialize',
              params: {
                protocolVersion: '2026-01-26',
                clientInfo: { name: 'drasi-admin-ui', version: '1.0.0' },
                capabilities: {},
                appCapabilities: { availableDisplayModes: ['inline', 'fullscreen'] }
              }
            },
            '*'
          );
        } catch (e) {}

        // Fallback: if the host never answers (older / non-conforming host),
        // assume unbounded and grow anyway so the UI isn't stuck at ~150px.
        setTimeout(function () {
          if (!handshakeDone) {
            sizeMode = sizeMode || 'unbounded';
            reportSize();
          }
        }, 1500);
      } catch (e) {
        try { console.error('drasi mcp host handshake failed', e); } catch (e2) {}
      }
    })();

    // --- Diagnostics: report sandbox DOM state + errors back to the server so
    // they surface in the MCP server log (the sandbox console is not otherwise
    // observable). Remove once MCP App rendering is confirmed working.
    var __diag = { errors: [] };
    function __report(phase) {
      try {
        var root = document.getElementById('root');
        var b = document.body;
        var cs = b && window.getComputedStyle ? window.getComputedStyle(b) : null;
        var payload = {
          phase: phase,
          href: location.href,
          readyState: document.readyState,
          rootExists: !!root,
          rootChildren: root ? root.childElementCount : -1,
          rootHtmlLen: root ? root.innerHTML.length : -1,
          bodyW: b ? b.clientWidth : -1,
          bodyH: b ? b.clientHeight : -1,
          docH: document.documentElement ? document.documentElement.scrollHeight : -1,
          innerW: window.innerWidth,
          innerH: window.innerHeight,
          bg: cs ? cs.backgroundColor : '',
          htmlClass: document.documentElement ? document.documentElement.className : '',
          errors: __diag.errors.slice(0, 12)
        };
        var msg = JSON.stringify(payload);
        if (navigator.sendBeacon) { navigator.sendBeacon(base + '/__mcp/diag', msg); }
        else if (window.fetch) { fetch(base + '/__mcp/diag', { method: 'POST', body: msg }); }
      } catch (e) {}
    }
    window.addEventListener('error', function (ev) {
      try {
        __diag.errors.push(String(ev.message) + ' @ ' + (ev.filename || '') + ':' + (ev.lineno || ''));
      } catch (e) {}
    });
    window.addEventListener('unhandledrejection', function (ev) {
      try { __diag.errors.push('promise: ' + String(ev.reason)); } catch (e) {}
    });
    window.addEventListener('load', function () {
      __report('load');
      setTimeout(function () { __report('load+3s'); }, 3000);
    });
    setTimeout(function () { __report('t6s'); }, 6000);

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

/// Serve the MCP App bridge script (see [`MCP_BRIDGE_JS`]).
pub async fn serve_mcp_bridge() -> Response {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(MCP_BRIDGE_JS))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

/// Path at which the MCP App bridge posts diagnostics.
pub const MCP_DIAG_PATH: &str = "/__mcp/diag";

/// Receive a diagnostic snapshot posted by the MCP App bridge and log it (so it
/// surfaces in the MCP server log). Diagnoses why a fully-loaded SPA may render
/// blank inside the host sandbox.
pub async fn serve_mcp_diag(body: String) -> Response {
    log::info!("[mcp-diag] {body}");
    Response::builder()
        .status(StatusCode::NO_CONTENT)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::empty())
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
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
