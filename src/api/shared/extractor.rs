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

//! Request-body extraction with JSON/YAML content negotiation.
//!
//! This module hosts [`ConfigBody`], a custom Axum extractor that lets every
//! body-accepting API route consume JSON or YAML interchangeably, selected from
//! the request's `Content-Type` header. It is intentionally separate from
//! `error.rs` (which owns `ErrorResponse` and error codes) because its primary
//! concern is content negotiation, not error handling.

use axum::async_trait;
use axum::body::Bytes;
use axum::extract::FromRequest;
use axum::http::header::CONTENT_TYPE;
use axum::http::StatusCode;
use serde::de::DeserializeOwned;

use super::error::{error_codes, ErrorDetail, ErrorResponse};

/// A request-body extractor that accepts both JSON and YAML payloads.
///
/// The body format is selected from the request's `Content-Type` header:
/// YAML media types (`application/yaml`, `application/x-yaml`, `text/yaml`,
/// `text/x-yaml`, `text/vnd.yaml`) are parsed with `serde_yaml`; everything
/// else (including a missing `Content-Type`) defaults to JSON. This lets every
/// HTTP route on the API accept JSON and YAML interchangeably.
///
/// On failure it returns a `(StatusCode, Json<ErrorResponse>)` rejection. Parse
/// failures surface as `400 INVALID_REQUEST` with the underlying serde
/// diagnostic in `ErrorDetail::technical_details`; body-read failures preserve
/// the status code reported by the inner `Bytes` extractor (for example, a
/// `413 Payload Too Large` from the configured body-size limit).
#[derive(Debug, Clone, Copy, Default)]
pub struct ConfigBody<T>(pub T);

/// Returns `true` when the supplied `Content-Type` value denotes a YAML media type.
pub(crate) fn is_yaml_content_type(content_type: &str) -> bool {
    // Ignore any parameters (e.g. "; charset=utf-8") and surrounding whitespace.
    let essence = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    matches!(
        essence.as_str(),
        "application/yaml" | "application/x-yaml" | "text/yaml" | "text/x-yaml" | "text/vnd.yaml"
    )
}

/// Detects YAML anchor (`&name`) or alias (`*name`) tokens.
///
/// `serde_yaml` expands anchors/aliases eagerly during deserialization, so a
/// small "billion laughs" document can balloon into an enormous in-memory
/// structure and exhaust the heap. API configuration payloads never need YAML
/// anchors, so we reject any document that uses them before parsing.
///
/// The scan is structure-aware to avoid false positives on ordinary scalar
/// values: it skips single/double-quoted regions and `#` comments, and only
/// treats `&`/`*` as a node tag when it begins a token (preceded by a
/// structural boundary) and is immediately followed by an anchor-name
/// character. As a result values such as `name: Tom & Jerry`, `id: AT&T`, or
/// `expr: "a * b"` are *not* flagged. (Literal block scalars that embed
/// `&word`/`*word` at a line start are a rare residual edge case; such payloads
/// can be sent as JSON instead.)
pub(crate) fn contains_yaml_anchor_or_alias(text: &str) -> bool {
    let bytes = text.as_bytes();
    let mut i = 0;
    // Start-of-input counts as a structural boundary.
    let mut at_boundary = true;

    while i < bytes.len() {
        match bytes[i] {
            b'#' => {
                // Comment: skip to end of line.
                while i < bytes.len() && bytes[i] != b'\n' {
                    i += 1;
                }
                at_boundary = true;
            }
            b'\'' => {
                // Single-quoted scalar: '' is an escaped quote.
                i += 1;
                while i < bytes.len() {
                    if bytes[i] == b'\'' {
                        if bytes.get(i + 1) == Some(&b'\'') {
                            i += 2;
                            continue;
                        }
                        break;
                    }
                    i += 1;
                }
                i += 1; // step past the closing quote
                at_boundary = false;
                continue;
            }
            b'"' => {
                // Double-quoted scalar: \" is an escaped quote.
                i += 1;
                while i < bytes.len() {
                    match bytes[i] {
                        b'\\' => i += 2,
                        b'"' => break,
                        _ => i += 1,
                    }
                }
                i += 1; // step past the closing quote
                at_boundary = false;
                continue;
            }
            b'&' | b'*' => {
                if at_boundary {
                    if let Some(&next) = bytes.get(i + 1) {
                        if next.is_ascii_alphanumeric() || next == b'_' || next == b'-' {
                            return true;
                        }
                    }
                }
                at_boundary = false;
                i += 1;
            }
            // Structural boundaries after which a node (and thus an anchor or
            // alias) may begin.
            b' ' | b'\t' | b'\r' | b'\n' | b'-' | b':' | b',' | b'[' | b'{' => {
                at_boundary = true;
                i += 1;
            }
            _ => {
                at_boundary = false;
                i += 1;
            }
        }
    }

    false
}

fn parse_error(format: &str, e: impl std::fmt::Display) -> (StatusCode, axum::Json<ErrorResponse>) {
    ErrorResponse::new(
        error_codes::INVALID_REQUEST,
        format!("Failed to parse {format} request body"),
    )
    .with_details(ErrorDetail {
        component_type: None,
        component_id: None,
        technical_details: Some(e.to_string()),
    })
    .with_status()
}

#[async_trait]
impl<T, S> FromRequest<S> for ConfigBody<T>
where
    T: DeserializeOwned,
    S: Send + Sync,
{
    /// A `(StatusCode, Json<ErrorResponse>)` tuple so that every rejection
    /// branch (body-read vs. parse failures) reports a consistent, structured
    /// error while still being able to preserve the inner extractor's status.
    type Rejection = (StatusCode, axum::Json<ErrorResponse>);

    async fn from_request(
        req: axum::http::Request<axum::body::Body>,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let is_yaml = req
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(is_yaml_content_type)
            .unwrap_or(false);

        let bytes = Bytes::from_request(req, state).await.map_err(|rejection| {
            // Preserve the inner extractor's status (e.g. 413 Payload Too
            // Large) rather than collapsing every body-read failure to 400.
            let status = rejection.status();
            log::debug!("Failed to read request body: {}", rejection.body_text());
            let body =
                ErrorResponse::new(error_codes::INVALID_REQUEST, "Failed to read request body")
                    .with_details(ErrorDetail {
                        component_type: None,
                        component_id: None,
                        technical_details: Some(rejection.body_text()),
                    });
            (status, axum::Json(body))
        })?;

        if is_yaml {
            // Reject anchor/alias-based expansion attacks before serde_yaml
            // eagerly expands them.
            if let Ok(text) = std::str::from_utf8(&bytes) {
                if contains_yaml_anchor_or_alias(text) {
                    return Err(ErrorResponse::new(
                        error_codes::INVALID_REQUEST,
                        "YAML anchors and aliases are not permitted in request bodies",
                    )
                    .with_status());
                }
            }
            serde_yaml::from_slice(&bytes).map(ConfigBody).map_err(|e| {
                log::debug!("YAML extraction failed: {e}");
                parse_error("YAML", e)
            })
        } else {
            serde_json::from_slice(&bytes).map(ConfigBody).map_err(|e| {
                log::debug!("JSON extraction failed: {e}");
                parse_error("JSON", e)
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn yaml_content_types_are_recognised() {
        for ct in [
            "application/yaml",
            "application/x-yaml",
            "text/yaml",
            "text/x-yaml",
            "text/vnd.yaml",
        ] {
            assert!(is_yaml_content_type(ct), "{ct} should be YAML");
        }
    }

    #[test]
    fn yaml_content_types_ignore_parameters_and_case() {
        assert!(is_yaml_content_type("Application/YAML; charset=utf-8"));
        assert!(is_yaml_content_type("  text/yaml ; foo=bar"));
        assert!(is_yaml_content_type("TEXT/VND.YAML"));
    }

    #[test]
    fn non_yaml_content_types_are_rejected() {
        for ct in [
            "application/json",
            "text/plain",
            "application/xml",
            "application/octet-stream",
            "",
        ] {
            assert!(!is_yaml_content_type(ct), "{ct} should not be YAML");
        }
    }

    #[test]
    fn detects_anchors_and_aliases() {
        assert!(contains_yaml_anchor_or_alias("a: &anchor 1\nb: *anchor\n"));
        assert!(contains_yaml_anchor_or_alias("items: [&x 1, *x]"));
        assert!(contains_yaml_anchor_or_alias("- &node\n  k: v"));
        // Plain unquoted scalar starting with `*` is a YAML alias.
        assert!(contains_yaml_anchor_or_alias("ref: *alias"));
    }

    #[test]
    fn does_not_flag_ampersand_or_star_in_plain_scalars() {
        assert!(!contains_yaml_anchor_or_alias("name: Tom & Jerry"));
        assert!(!contains_yaml_anchor_or_alias("company: AT&T"));
        assert!(!contains_yaml_anchor_or_alias("note: see * for footnote"));
        assert!(!contains_yaml_anchor_or_alias("glob: *.log"));
        assert!(!contains_yaml_anchor_or_alias(
            "id: high-temp\nq: MATCH (n)"
        ));
    }

    #[test]
    fn does_not_flag_quoted_or_commented_tokens() {
        assert!(!contains_yaml_anchor_or_alias("query: \"RETURN *\""));
        assert!(!contains_yaml_anchor_or_alias("expr: 'a & b and *c'"));
        assert!(!contains_yaml_anchor_or_alias(
            "k: v  # &anchor *alias here"
        ));
    }
}
