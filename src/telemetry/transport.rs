// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use super::*;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceResponse;
use prost::Message;
use std::sync::Mutex;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum OtlpSignal {
    Metrics,
    Traces,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct TraceExportResponse {
    pub rejected_spans: u64,
    pub has_warning: bool,
}

/// Replaces SDK environment-derived routing/headers with the resolved policy.
#[derive(Clone)]
pub(super) struct ResolvedOtlpHttpClient {
    pub(super) client: reqwest_otel::blocking::Client,
    pub(super) destination: crate::config::telemetry::OtlpDestination,
    timeout: Duration,
    deadline: Arc<Mutex<Instant>>,
    signal: OtlpSignal,
    trace_response: Arc<Mutex<Option<TraceExportResponse>>>,
}

impl std::fmt::Debug for ResolvedOtlpHttpClient {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("ResolvedOtlpHttpClient { protocol: http/protobuf, .. }")
    }
}

impl ResolvedOtlpHttpClient {
    pub(super) fn new(
        destination: crate::config::telemetry::OtlpDestination,
        timeout: Duration,
        signal: OtlpSignal,
    ) -> Result<Self> {
        let client = reqwest_otel::blocking::Client::builder()
            .timeout(timeout)
            .redirect(reqwest_otel::redirect::Policy::none())
            .build()
            .map_err(|_| anyhow::anyhow!("Cannot initialize resolved OTLP HTTP client"))?;
        Ok(Self {
            client,
            destination,
            timeout,
            deadline: Arc::new(Mutex::new(Instant::now())),
            signal,
            trace_response: Arc::new(Mutex::new(None)),
        })
    }

    pub(super) fn allow_until(&self, phase_deadline: Option<Instant>) -> OTelSdkResult {
        let request_deadline = Instant::now() + self.timeout;
        *self.deadline.lock().map_err(|_| {
            OTelSdkError::InternalFailure("OTLP request deadline lock failed".into())
        })? = phase_deadline.map_or(request_deadline, |deadline| deadline.min(request_deadline));
        Ok(())
    }

    pub(super) fn take_trace_response(
        &self,
    ) -> std::result::Result<TraceExportResponse, OTelSdkError> {
        self.trace_response
            .lock()
            .map_err(|_| OTelSdkError::InternalFailure("OTLP response lock failed".into()))?
            .take()
            .ok_or_else(|| {
                OTelSdkError::InternalFailure("OTLP trace response was not observed".into())
            })
    }
}

#[async_trait::async_trait]
impl opentelemetry_http::HttpClient for ResolvedOtlpHttpClient {
    async fn send_bytes(
        &self,
        request: opentelemetry_http::Request<bytes::Bytes>,
    ) -> std::result::Result<
        opentelemetry_http::Response<bytes::Bytes>,
        opentelemetry_http::HttpError,
    > {
        use std::io::Read;
        if self.signal == OtlpSignal::Traces {
            self.trace_response
                .lock()
                .map_err(|_| std::io::Error::other("OTLP response lock failed"))?
                .take();
        }
        let deadline = *self
            .deadline
            .lock()
            .map_err(|_| std::io::Error::other("OTLP request deadline lock failed"))?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::TimedOut,
                "OTLP request allowance elapsed",
            )
            .into());
        }
        let response = self
            .client
            .post(self.destination.endpoint().clone())
            .headers(self.destination.headers().clone())
            .header(axum::http::header::CONTENT_TYPE, "application/x-protobuf")
            .timeout(remaining)
            .body(request.into_body())
            .send()
            .map_err(|_| std::io::Error::other("OTLP HTTP request failed"))?;
        let status = response.status();
        let mut body = Vec::new();
        response
            .take(4 * 1024 * 1024 + 1)
            .read_to_end(&mut body)
            .map_err(|_| std::io::Error::other("OTLP HTTP response failed"))?;
        if body.len() > 4 * 1024 * 1024 {
            return Err(opentelemetry_http::ResponseBodyTooLarge::new().into());
        }
        if self.signal == OtlpSignal::Traces {
            if status.is_success() {
                let response = ExportTraceServiceResponse::decode(body.as_slice())
                    .map_err(|_| std::io::Error::other("Invalid OTLP trace response"))?;
                let (rejected_spans, has_warning) = match response.partial_success {
                    Some(partial) => (
                        u64::try_from(partial.rejected_spans)
                            .map_err(|_| std::io::Error::other("Invalid OTLP rejection count"))?,
                        !partial.error_message.is_empty(),
                    ),
                    None => (0, false),
                };
                *self
                    .trace_response
                    .lock()
                    .map_err(|_| std::io::Error::other("OTLP response lock failed"))? =
                    Some(TraceExportResponse {
                        rejected_spans,
                        has_warning,
                    });
            }
            // The pinned exporter logs collector-supplied response text.
            // Return only status after inspecting the bounded response ourselves.
            body.clear();
        }
        Ok(opentelemetry_http::Response::builder()
            .status(status)
            .body(body.into())?)
    }
}
