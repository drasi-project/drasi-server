// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use axum::http::{HeaderMap, HeaderValue};
use drasi_lib::observability::TraceContextV1;
use drasi_plugin_sdk::resolver::{EnvironmentVariableResolver, ValueResolver};
use drasi_plugin_sdk::ConfigValue;
use serde::{de::DeserializeOwned, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use crate::api::shared::error::{error_codes, ErrorDetail, ErrorResponse};

/// Resolves a proxy target property from a source's reported configuration.
///
/// Sources report their authoring configuration, so values may still be
/// environment references. The proxy never resolves Secret references, and an
/// unresolvable configured value fails instead of falling back to the default.
pub(crate) async fn target_property<T>(
    properties: &HashMap<String, serde_json::Value>,
    key: &str,
    default: T,
) -> Result<T, Box<ErrorResponse>>
where
    T: Serialize + DeserializeOwned + Clone + FromStr + 'static,
{
    let Some(value) = properties.get(key) else {
        return Ok(default);
    };
    let unavailable = |detail: String| {
        Box::new(
            ErrorResponse::new(
                error_codes::INTERNAL_ERROR,
                "Source proxy target unavailable",
            )
            .with_details(ErrorDetail {
                component_type: Some("source".to_string()),
                component_id: None,
                technical_details: Some(format!("{key}: {detail}")),
            }),
        )
    };
    match serde_json::from_value::<ConfigValue<T>>(value.clone()) {
        Ok(ConfigValue::Static(value)) => Ok(value),
        Ok(ConfigValue::EnvironmentVariable { name, default }) => EnvironmentVariableResolver
            .resolve_to_string(&ConfigValue::EnvironmentVariable { name, default })
            .await
            .map_err(|error| unavailable(error.to_string()))?
            .parse()
            .map_err(|_| unavailable("resolved value is invalid".to_string())),
        Ok(ConfigValue::Secret { .. }) => Err(unavailable(
            "Secret references are not resolved by the source proxy".to_string(),
        )),
        Err(error) => Err(unavailable(error.to_string())),
    }
}

/// Shared, redirect-disabled client for the source-data proxy handler.
#[derive(Clone, Default)]
pub struct SourceProxyClient(Arc<OnceLock<Result<reqwest::Client, reqwest::Error>>>);

impl SourceProxyClient {
    pub(crate) fn client(&self) -> Result<&reqwest::Client, Box<ErrorResponse>> {
        self.0
            .get_or_init(|| {
                reqwest::Client::builder()
                    .redirect(reqwest::redirect::Policy::none())
                    .build()
            })
            .as_ref()
            .map_err(|error| {
                log::error!("Cannot initialize source-data proxy transport");
                Box::new(
                    ErrorResponse::new(error_codes::INTERNAL_ERROR, "Source proxy unavailable")
                        .with_details(ErrorDetail {
                            component_type: None,
                            component_id: None,
                            technical_details: Some(error.to_string()),
                        }),
                )
            })
    }
}

fn invalid_context() -> HeaderMap {
    static LAST_WARNING: Mutex<Option<Instant>> = Mutex::new(None);
    match LAST_WARNING.lock() {
        Ok(mut last) if last.is_none_or(|last| last.elapsed() >= Duration::from_secs(30)) => {
            *last = Some(Instant::now());
            log::warn!(
                "Ignoring invalid source-proxy W3C context; forwarding data without trace metadata"
            );
        }
        Ok(_) => {}
        Err(_) => log::error!("Source-proxy trace diagnostic state is poisoned"),
    }
    HeaderMap::new()
}

pub(crate) fn validated_trace_headers(headers: &HeaderMap) -> HeaderMap {
    let mut parents = headers.get_all("traceparent").iter();
    let Some(parent) = parents.next() else {
        return HeaderMap::new();
    };
    if parents.next().is_some() || parent.as_bytes().len() > 512 {
        return invalid_context();
    }
    let mut state = Vec::new();
    for value in headers.get_all("tracestate") {
        let separator = usize::from(!state.is_empty());
        if value.as_bytes().len() > 512usize.saturating_sub(state.len() + separator) {
            return invalid_context();
        }
        if separator != 0 {
            state.push(b',');
        }
        state.extend_from_slice(value.as_bytes());
    }
    let state_bytes = (!state.is_empty()).then_some(state.as_slice());
    if TraceContextV1::from_w3c_headers(parent.as_bytes(), state_bytes).is_err() {
        return invalid_context();
    }
    let mut forwarded = HeaderMap::new();
    forwarded.insert("traceparent", parent.clone());
    if let Some(state) = state_bytes {
        match HeaderValue::from_bytes(state) {
            Ok(value) => {
                forwarded.insert("tracestate", value);
            }
            Err(_) => return invalid_context(),
        }
    }
    forwarded
}

#[cfg(test)]
mod tests {
    use super::validated_trace_headers;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        Router,
    };
    use drasi_lib::observability::{ComponentLogRuntime, LogPolicy};
    use drasi_source_application::{ApplicationSource, ApplicationSourceConfig};
    use serde_json::json;
    use std::{sync::Arc, time::Duration};
    use tokio::sync::RwLock;
    use tower::ServiceExt;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    const PARENT: &str = "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01";

    #[test]
    fn proxy_rejects_ambiguous_malformed_and_oversized_context() {
        use axum::http::{HeaderMap, HeaderValue};
        for parent in [
            "malformed".to_owned(),
            PARENT.replace("4bf92", "4BF92"),
            PARENT.replace(
                "4bf92f3577b34da6a3ce929d0e0e4736",
                "00000000000000000000000000000000",
            ),
            "x".repeat(513),
        ] {
            let mut headers = HeaderMap::new();
            headers.insert("traceparent", HeaderValue::from_str(&parent).unwrap());
            assert!(validated_trace_headers(&headers).is_empty());
        }
        let mut headers = HeaderMap::new();
        headers.append("traceparent", HeaderValue::from_static(PARENT));
        headers.append("traceparent", HeaderValue::from_static(PARENT));
        assert!(validated_trace_headers(&headers).is_empty());
        headers.remove("traceparent");
        headers.insert("tracestate", HeaderValue::from_static("vendor=value"));
        assert!(validated_trace_headers(&headers).is_empty());
    }

    #[test]
    fn proxy_header_byte_and_member_limits_are_enforced_before_forwarding() {
        use axum::http::{HeaderMap, HeaderValue};
        let mut headers = HeaderMap::new();
        let future = format!("01{}-{}", &PARENT[2..], "x".repeat(456));
        assert_eq!(future.len(), 512);
        headers.insert("traceparent", HeaderValue::from_str(&future).unwrap());
        assert_eq!(validated_trace_headers(&headers)["traceparent"], future);
        headers.insert(
            "traceparent",
            HeaderValue::from_str(&(future + "x")).unwrap(),
        );
        assert!(validated_trace_headers(&headers).is_empty());
        headers.insert("traceparent", HeaderValue::from_static(PARENT));
        let state = format!("a={},b={}", "x".repeat(253), "x".repeat(254));
        assert_eq!(state.len(), 512);
        headers.insert("tracestate", HeaderValue::from_str(&state).unwrap());
        assert_eq!(validated_trace_headers(&headers)["tracestate"], state);
        headers.insert("tracestate", HeaderValue::from_str(&(state + "x")).unwrap());
        assert!(validated_trace_headers(&headers).is_empty());
        for (members, valid) in [(32, true), (33, false)] {
            let state = (0..members)
                .map(|id| format!("v{id}=x"))
                .collect::<Vec<_>>()
                .join(",");
            headers.insert("tracestate", HeaderValue::from_str(&state).unwrap());
            assert_eq!(!validated_trace_headers(&headers).is_empty(), valid);
        }
    }

    #[tokio::test]
    async fn proxy_forwards_only_validated_w3c_headers() -> anyhow::Result<()> {
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"accepted":true})))
            .mount(&receiver)
            .await;
        let (router, core, logging, id) = setup(&receiver).await?;
        assert!(!core.telemetry().policy().enabled());
        let response = router
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/instances/proxy/sources/{id}/push"))
                    .header("content-type", "application/json")
                    .header("traceparent", PARENT)
                    .header("tracestate", "first=one")
                    .header("tracestate", "second=two")
                    .header("authorization", "must-not-forward")
                    .header("baggage", "private=value")
                    .body(Body::from(r#"{"value":7}"#))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        for parent in [
            Some("malformed"),
            None,
            Some("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00"),
        ] {
            let mut request = Request::builder()
                .method("POST")
                .uri(format!("/api/v1/instances/proxy/sources/{id}/push"))
                .header("content-type", "application/json");
            if let Some(parent) = parent {
                request = request.header("traceparent", parent);
            }
            assert_eq!(
                router
                    .clone()
                    .oneshot(request.body(Body::from(r#"{"value":7}"#))?)
                    .await?
                    .status(),
                StatusCode::OK
            );
        }
        let requests = receiver.received_requests().await.unwrap();
        assert_eq!(requests.len(), 4);
        assert_eq!(requests[0].headers["traceparent"], PARENT);
        assert_eq!(requests[0].headers["tracestate"], "first=one,second=two");
        assert!(!requests[0].headers.contains_key("authorization"));
        assert!(!requests[0].headers.contains_key("baggage"));
        for request in &requests[1..3] {
            assert!(!request.headers.contains_key("traceparent"));
            assert!(!request.headers.contains_key("tracestate"));
        }
        assert!(requests[3].headers["traceparent"]
            .to_str()?
            .ends_with("-00"));
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&requests[0].body)?,
            json!({"value":7})
        );
        core.shutdown().await?;
        logging.shutdown(Duration::from_secs(3)).await?;
        Ok(())
    }

    #[tokio::test]
    async fn proxy_context_does_not_follow_redirect() -> anyhow::Result<()> {
        let target = MockServer::start().await;
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(307).insert_header("location", target.uri()))
            .mount(&receiver)
            .await;
        let (router, core, logging, id) = setup(&receiver).await?;
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/instances/proxy/sources/{id}/push"))
                    .header("content-type", "application/json")
                    .header("traceparent", PARENT)
                    .body(Body::from("{}"))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert!(target.received_requests().await.unwrap().is_empty());
        core.shutdown().await?;
        logging.shutdown(Duration::from_secs(3)).await?;
        Ok(())
    }

    #[tokio::test]
    async fn proxy_forwards_to_the_assigned_source_id_path() -> anyhow::Result<()> {
        // Sources validate the path against their Core-assigned UUID, not the name.
        let receiver = MockServer::start().await;
        let (router, core, logging, name) = setup(&receiver).await?;
        let assigned = core.component_by_name(&name)?.id().to_string();
        Mock::given(method("POST"))
            .and(wiremock::matchers::path(format!(
                "/sources/{assigned}/events"
            )))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"accepted":true})))
            .mount(&receiver)
            .await;
        let response = router
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri(format!("/api/v1/instances/proxy/sources/{name}/push"))
                    .header("content-type", "application/json")
                    .body(Body::from(r#"{"value":1}"#))?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        core.shutdown().await?;
        logging.shutdown(Duration::from_secs(3)).await?;
        Ok(())
    }

    fn push(name: &str) -> anyhow::Result<Request<Body>> {
        Ok(Request::builder()
            .method("POST")
            .uri(format!("/api/v1/instances/proxy/sources/{name}/push"))
            .header("content-type", "application/json")
            .header("traceparent", PARENT)
            .body(Body::from(r#"{"value":1}"#))?)
    }

    #[tokio::test]
    async fn proxy_resolves_environment_referenced_target() -> anyhow::Result<()> {
        // Plugins receive authoring config, so properties keep unresolved references.
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_json(json!({"accepted":true})))
            .mount(&receiver)
            .await;
        let port = receiver.address().port();
        for (host, port) in [
            (
                json!("${DRASI_PROXY_TEST_UNSET_HOST:-127.0.0.1}"),
                json!(format!("${{DRASI_PROXY_TEST_UNSET_PORT:-{port}}}")),
            ),
            (
                json!({"kind": "EnvironmentVariable", "name": "DRASI_PROXY_TEST_UNSET_HOST", "default": "127.0.0.1"}),
                json!({"kind": "EnvironmentVariable", "name": "DRASI_PROXY_TEST_UNSET_PORT", "default": port.to_string()}),
            ),
        ] {
            let (router, core, logging, name) = setup_with(host, port).await?;
            assert_eq!(router.oneshot(push(&name)?).await?.status(), StatusCode::OK);
            core.shutdown().await?;
            logging.shutdown(Duration::from_secs(3)).await?;
        }
        let requests = receiver.received_requests().await.unwrap();
        assert_eq!(requests.len(), 2);
        assert!(requests.iter().all(|r| r.headers["traceparent"] == PARENT));
        Ok(())
    }

    #[tokio::test]
    async fn proxy_rejects_unresolvable_target_without_default_port_fallback() -> anyhow::Result<()>
    {
        let receiver = MockServer::start().await;
        for port in [
            json!({"kind": "Secret", "name": "source-port"}),
            json!("${DRASI_PROXY_TEST_UNSET_PORT}"),
            json!("${DRASI_PROXY_TEST_UNSET_PORT:-70000}"),
            json!("not-a-port"),
        ] {
            let (router, core, logging, name) =
                setup_with(json!("127.0.0.1"), port.clone()).await?;
            let response = router.oneshot(push(&name)?).await?;
            assert_eq!(
                response.status(),
                StatusCode::INTERNAL_SERVER_ERROR,
                "{port}"
            );
            let body: serde_json::Value = serde_json::from_slice(
                &axum::body::to_bytes(response.into_body(), usize::MAX).await?,
            )?;
            assert_eq!(body["message"], "Source proxy target unavailable", "{port}");
            core.shutdown().await?;
            logging.shutdown(Duration::from_secs(3)).await?;
        }
        assert!(receiver.received_requests().await.unwrap().is_empty());
        Ok(())
    }

    async fn setup(
        receiver: &MockServer,
    ) -> anyhow::Result<(
        Router,
        Arc<drasi_lib::DrasiLib>,
        ComponentLogRuntime,
        String,
    )> {
        let port = receiver.address().port();
        setup_with(json!("127.0.0.1"), json!(port)).await
    }

    async fn setup_with(
        host: serde_json::Value,
        port: serde_json::Value,
    ) -> anyhow::Result<(
        Router,
        Arc<drasi_lib::DrasiLib>,
        ComponentLogRuntime,
        String,
    )> {
        let logging = ComponentLogRuntime::new(LogPolicy::new("off")?)?;
        let core = Arc::new(
            drasi_lib::DrasiLib::builder()
                .with_id("proxy")
                .with_component_logging(logging.clone())
                .with_source_factory("input", false, move |id| {
                    let (source, _) = ApplicationSource::new(
                        id.as_str(),
                        ApplicationSourceConfig {
                            properties: std::collections::HashMap::from([
                                ("host".into(), host.clone()),
                                ("port".into(), port.clone()),
                            ]),
                            durability: None,
                        },
                    )?;
                    Ok(source)
                })
                .build()
                .await?,
        );
        let component = core.component_by_name("input")?;
        tokio::time::timeout(Duration::from_secs(3), component.wait_created()).await??;
        // The push route resolves its path segment by source name.
        let id = "input".to_string();
        let registry = crate::instance_registry::InstanceRegistry::new();
        registry
            .add("proxy".into(), core.clone())
            .await
            .map_err(anyhow::Error::msg)?;
        let router = Router::new().nest(
            "/api/v1",
            crate::api::v1::routes::build_v1_router(
                registry,
                Arc::new(false),
                None,
                Arc::new(RwLock::new(crate::plugin_registry::PluginRegistry::new())),
                None,
            ),
        );
        Ok((router, core, logging, id))
    }
}
