// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

use anyhow::{Context, Result};
use drasi_lib::{
    context::workers::try_spawn_owned_worker,
    observability::{
        NativeSpanCollector, NativeTelemetryError, OpenTelemetryContextAdapter,
        OpenTelemetrySpanProcessor, RootSampling, TraceContextAdapter,
    },
};
use opentelemetry::{trace::TracerProvider, Array, KeyValue, Value};
use opentelemetry_otlp::{Protocol, WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::{
    error::{OTelSdkError, OTelSdkResult},
    trace::{
        Sampler, SdkTracer, SdkTracerProvider, Span, SpanData, SpanExporter, SpanLimits,
        SpanProcessor,
    },
    Resource,
};
use std::{
    collections::VecDeque,
    mem::size_of,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex, MutexGuard,
    },
    time::Duration,
};
use tokio::{
    sync::{oneshot, Notify, RwLock},
    task::JoinHandle,
    time::{timeout_at, Instant},
};

use super::{
    transport::{OtlpSignal, ResolvedOtlpHttpClient},
    ExportWarnings,
};
use crate::config::telemetry::EffectiveTelemetry;

const MAX_SPANS: usize = 2_048;
const MAX_BYTES: usize = 16 * 1024 * 1024;
const BATCH_SIZE: usize = 256;
const EXPORT_DELAY: Duration = Duration::from_secs(1);
const EXPORT_TIMEOUT: Duration = Duration::from_secs(5);

#[derive(Clone)]
pub(crate) struct TraceCollection {
    pub tracer: SdkTracer,
    pub context_adapter: Arc<dyn TraceContextAdapter>,
    pub native_spans: NativeSpanCollector,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct TraceLossSnapshot {
    pub exported: u64,
    pub full: u64,
    pub oversized: u64,
    pub closed: u64,
    pub export_failed: u64,
    pub collector_rejected: u64,
    pub shutdown_discarded: u64,
}

struct Pending {
    span: SpanData,
    bytes: usize,
}

struct Queue {
    open: bool,
    active: bool,
    native_finished: bool,
    cleanup_requested: bool,
    deadline: Option<Instant>,
    records: usize,
    bytes: usize,
    pending: VecDeque<Pending>,
    losses: TraceLossSnapshot,
}

struct Shared {
    queue: Mutex<Queue>,
    changed: Condvar,
}

impl Shared {
    fn new() -> Self {
        Self {
            queue: Mutex::new(Queue {
                open: true,
                active: false,
                native_finished: false,
                cleanup_requested: false,
                deadline: None,
                records: 0,
                bytes: 0,
                pending: VecDeque::new(),
                losses: Default::default(),
            }),
            changed: Condvar::new(),
        }
    }

    fn lock(&self) -> Result<MutexGuard<'_, Queue>> {
        self.queue
            .lock()
            .map_err(|_| anyhow::anyhow!("Trace queue lock failed"))
    }

    fn admit(&self, span: SpanData) -> Result<bool> {
        let bytes = accounted_size(&span);
        let mut queue = self.lock()?;
        if !queue.open {
            queue.losses.closed = queue.losses.closed.saturating_add(1);
            return Ok(false);
        }
        let Some(bytes) = bytes.filter(|bytes| *bytes <= MAX_BYTES) else {
            queue.losses.oversized = queue.losses.oversized.saturating_add(1);
            return Ok(false);
        };
        if queue.records >= MAX_SPANS || bytes > MAX_BYTES.saturating_sub(queue.bytes) {
            queue.losses.full = queue.losses.full.saturating_add(1);
            return Ok(false);
        }
        queue.records += 1;
        queue.bytes += bytes;
        queue.pending.push_back(Pending { span, bytes });
        self.changed.notify_one();
        Ok(true)
    }

    #[cfg(test)]
    fn retained(&self) -> Result<(usize, usize)> {
        let queue = self.lock()?;
        Ok((queue.records, queue.bytes))
    }

    fn losses(&self) -> Result<TraceLossSnapshot> {
        Ok(self.lock()?.losses)
    }

    fn take_batch(&self) -> Result<Vec<Pending>> {
        let mut queue = self.lock()?;
        let count = queue.pending.len().min(BATCH_SIZE);
        Ok(queue.pending.drain(..count).collect())
    }

    #[cfg(test)]
    fn release(&self, batch: &[Pending]) -> Result<()> {
        let bytes: usize = batch.iter().map(|pending| pending.bytes).sum();
        let mut queue = self.lock()?;
        queue.records = queue
            .records
            .checked_sub(batch.len())
            .context("Trace record reservation underflow")?;
        queue.bytes = queue
            .bytes
            .checked_sub(bytes)
            .context("Trace byte reservation underflow")?;
        self.changed.notify_all();
        Ok(())
    }

    fn request_stop(&self, deadline: Instant) -> Result<()> {
        let mut queue = self.lock()?;
        queue.deadline.get_or_insert(deadline);
        self.changed.notify_all();
        Ok(())
    }

    fn close(&self) -> Result<()> {
        let mut queue = self.lock()?;
        queue.open = false;
        queue.native_finished = true;
        self.changed.notify_all();
        Ok(())
    }
}

// Account the retained record plus every variable-length value, including
// nested attributes. This is not an allocator/RSS measurement.
fn accounted_size(span: &SpanData) -> Option<usize> {
    fn attributes<'a>(mut values: impl Iterator<Item = &'a KeyValue>) -> Option<usize> {
        values.try_fold(0usize, |total, attribute| {
            let payload = match &attribute.value {
                Value::Bool(_) | Value::I64(_) | Value::F64(_) => 0,
                Value::String(value) => value.as_str().len(),
                Value::Array(values) => match values {
                    Array::Bool(values) => values.len().checked_mul(size_of::<bool>())?,
                    Array::I64(values) => values.len().checked_mul(size_of::<i64>())?,
                    Array::F64(values) => values.len().checked_mul(size_of::<f64>())?,
                    Array::String(values) => values.iter().try_fold(0usize, |sum, value| {
                        sum.checked_add(size_of::<opentelemetry::StringValue>())?
                            .checked_add(value.as_str().len())
                    })?,
                    _ => return None,
                },
                _ => return None,
            };
            total
                .checked_add(size_of::<KeyValue>())?
                .checked_add(attribute.key.as_str().len())?
                .checked_add(payload)
        })
    }
    let scope = &span.instrumentation_scope;
    let mut bytes = size_of::<Pending>()
        .checked_add(span.name.len())?
        .checked_add(scope.name().len())?
        .checked_add(scope.version().map_or(0, str::len))?
        .checked_add(scope.schema_url().map_or(0, str::len))?
        .checked_add(span.span_context.trace_state().header().len())?
        .checked_add(attributes(span.attributes.iter())?)?
        .checked_add(attributes(scope.attributes())?)?;
    if let opentelemetry::trace::Status::Error { description } = &span.status {
        bytes = bytes.checked_add(description.len())?;
    }
    for event in &span.events.events {
        bytes = bytes
            .checked_add(size_of::<opentelemetry::trace::Event>())?
            .checked_add(event.name.len())?
            .checked_add(attributes(event.attributes.iter())?)?;
    }
    for link in &span.links.links {
        bytes = bytes
            .checked_add(size_of::<opentelemetry::trace::Link>())?
            .checked_add(link.span_context.trace_state().header().len())?
            .checked_add(attributes(link.attributes.iter())?)?;
    }
    Some(bytes)
}

#[derive(Clone)]
struct QueueSpanProcessor(Arc<Shared>);

impl std::fmt::Debug for QueueSpanProcessor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QueueSpanProcessor")
    }
}

impl SpanProcessor for QueueSpanProcessor {
    fn on_start(&self, _: &mut Span, _: &opentelemetry::Context) {}

    fn on_end(&self, span: SpanData) {
        if let Err(error) = self.0.admit(span) {
            log::error!("Trace admission failed: {error}");
        }
    }

    fn force_flush(&self) -> OTelSdkResult {
        let deadline = Instant::now() + EXPORT_TIMEOUT;
        let mut queue = self.0.lock().map_err(sdk_error)?;
        if !queue.active && queue.records != 0 {
            return Err(OTelSdkError::InternalFailure(
                "Trace export is not activated".into(),
            ));
        }
        self.0.changed.notify_one();
        while queue.records != 0 {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(OTelSdkError::Timeout(EXPORT_TIMEOUT));
            }
            queue = self
                .0
                .changed
                .wait_timeout(queue, remaining)
                .map_err(|_| OTelSdkError::InternalFailure("Trace queue lock failed".into()))?
                .0;
        }
        Ok(())
    }

    fn shutdown_with_timeout(&self, _: Duration) -> OTelSdkResult {
        self.0.close().map_err(sdk_error)
    }
}

fn sdk_error(_: anyhow::Error) -> OTelSdkError {
    OTelSdkError::InternalFailure("Trace collection state failed".into())
}

type Terminal = std::result::Result<(), Arc<anyhow::Error>>;
struct TraceHandles {
    provider: SdkTracerProvider,
    native_spans: NativeSpanCollector,
}

pub(crate) struct TraceRuntime {
    shared: Arc<Shared>,
    handles: Arc<Mutex<Option<TraceHandles>>>,
    initialized: Arc<AtomicBool>,
    initialization: Arc<Notify>,
    worker: RwLock<Option<JoinHandle<Terminal>>>,
    cleanup: RwLock<Option<JoinHandle<Terminal>>>,
    terminal: Mutex<Option<Terminal>>,
}

impl TraceRuntime {
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared::new()),
            handles: Default::default(),
            initialized: Arc::new(AtomicBool::new(false)),
            initialization: Default::default(),
            worker: RwLock::new(None),
            cleanup: RwLock::new(None),
            terminal: Mutex::new(None),
        }
    }

    pub async fn prepare(
        self: &Arc<Self>,
        effective: &EffectiveTelemetry,
        resource: Resource,
    ) -> Result<TraceCollection> {
        let destination = effective
            .tracing
            .clone()
            .context("Trace export requires a destination")?;
        let ratio = match effective.policy().sampling() {
            RootSampling::Always => 1.0,
            RootSampling::Never => 0.0,
            RootSampling::Ratio(ratio) => ratio.value(),
        };
        let mut worker = self
            .worker
            .try_write()
            .context("Trace preparation is busy")?;
        anyhow::ensure!(
            worker.is_none() && !self.initialized.load(Ordering::Acquire),
            "Trace runtime has already been prepared"
        );
        anyhow::ensure!(
            self.shared.lock()?.deadline.is_none(),
            "Trace runtime is stopping"
        );
        let shared = self.shared.clone();
        let handles = self.handles.clone();
        let initialized = self.initialized.clone();
        let initialization = self.initialization.clone();
        let (ready, receive) = oneshot::channel();
        *worker = Some(tokio::task::spawn_blocking(move || {
            struct Initialized(Arc<AtomicBool>, Arc<Notify>);
            impl Drop for Initialized {
                fn drop(&mut self) {
                    self.0.store(true, Ordering::Release);
                    self.1.notify_waiters();
                }
            }
            let init = Initialized(initialized, initialization);
            let prepared = (|| -> Result<_> {
                let transport =
                    ResolvedOtlpHttpClient::new(destination, EXPORT_TIMEOUT, OtlpSignal::Traces)?;
                let mut exporter = tracing::subscriber::with_default(
                    tracing::subscriber::NoSubscriber::default(),
                    || {
                        opentelemetry_otlp::SpanExporter::builder()
                            .with_http()
                            .with_http_client(transport.clone())
                            .with_protocol(Protocol::HttpBinary)
                            .with_endpoint(transport.destination.endpoint().as_str())
                            .with_timeout(EXPORT_TIMEOUT)
                            .with_retry_policy(
                                opentelemetry_otlp::RetryPolicy::default().with_max_retries(0),
                            )
                            .build()
                    },
                )
                .map_err(|_| anyhow::anyhow!("Cannot initialize resolved OTLP trace exporter"))?;
                exporter.set_resource(&resource);
                let processor = OpenTelemetrySpanProcessor::new(
                    QueueSpanProcessor(shared.clone()),
                    Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(ratio))),
                );
                let provider = SdkTracerProvider::builder()
                    .with_resource(resource)
                    .with_sampler(processor.sampler())
                    .with_span_processor(processor.clone())
                    .with_span_limits(SpanLimits {
                        max_attributes_per_span: 128,
                        max_events_per_span: 32,
                        max_links_per_span: 128,
                        max_attributes_per_event: 64,
                        max_attributes_per_link: 64,
                    })
                    .build();
                let native_spans =
                    NativeSpanCollector::new(Arc::new(processor), Default::default())?;
                let collection = TraceCollection {
                    tracer: provider.tracer("drasi.core"),
                    context_adapter: Arc::new(OpenTelemetryContextAdapter),
                    native_spans: native_spans.clone(),
                };
                *handles
                    .lock()
                    .map_err(|_| anyhow::anyhow!("Trace handle lock failed"))? =
                    Some(TraceHandles {
                        provider,
                        native_spans,
                    });
                Ok((exporter, transport, collection))
            })();
            drop(init);
            let (exporter, transport, collection) = match prepared {
                Ok(prepared) => prepared,
                Err(error) => {
                    let error = Arc::new(error);
                    let _ = ready.send(Err(anyhow::anyhow!("Trace initialization failed")));
                    return Err(error);
                }
            };
            if ready.send(Ok(collection)).is_err() {
                if let Err(error) = shared.request_stop(Instant::now()) {
                    return Err(Arc::new(error));
                }
            }
            run_exports(shared, exporter, transport).map_err(Arc::new)
        }));
        drop(worker);
        receive
            .await
            .context("Trace initialization worker exited")?
    }

    pub fn activate(&self) -> Result<()> {
        let mut queue = self.shared.lock()?;
        anyhow::ensure!(
            !queue.active && queue.deadline.is_none() && self.initialized.load(Ordering::Acquire),
            "Trace runtime is not prepared or is already active/stopping"
        );
        queue.active = true;
        self.shared.changed.notify_one();
        Ok(())
    }

    pub fn losses(&self) -> Result<TraceLossSnapshot> {
        self.shared.losses()
    }

    pub async fn request_shutdown(self: &Arc<Self>, deadline: Instant) -> Result<()> {
        self.shared.request_stop(deadline)?;
        let mut queue = self.shared.lock()?;
        if queue.cleanup_requested {
            return Ok(());
        }
        let owner = self.clone();
        try_spawn_owned_worker(&self.cleanup, async move {
            owner.finish_cleanup().await.map_err(Arc::new)
        })?;
        queue.cleanup_requested = true;
        Ok(())
    }

    async fn finish_cleanup(&self) -> Result<()> {
        if self.worker.read().await.is_none() {
            return Ok(());
        }
        while !self.initialized.load(Ordering::Acquire) {
            let notified = self.initialization.notified();
            tokio::pin!(notified);
            notified.as_mut().enable();
            if !self.initialized.load(Ordering::Acquire) {
                notified.await;
            }
        }
        let deadline = self
            .shared
            .lock()?
            .deadline
            .context("Missing trace shutdown deadline")?;
        let handles = self
            .handles
            .lock()
            .map_err(|_| anyhow::anyhow!("Trace handle lock failed"))?
            .as_ref()
            .map(|handles| (handles.provider.clone(), handles.native_spans.clone()));
        let mut failures = Vec::new();
        if let Some((provider, collector)) = handles {
            let mut allowance = deadline.saturating_duration_since(Instant::now());
            let mut expired = false;
            loop {
                match collector.shutdown(allowance).await {
                    Ok(()) => break,
                    Err(error @ NativeTelemetryError::TimedOut(_)) => {
                        if !expired {
                            failures.push(error.into());
                            expired = true;
                        }
                        allowance = Duration::from_secs(30);
                    }
                    Err(error) => {
                        failures.push(error.into());
                        break;
                    }
                }
            }
            self.shared.close()?;
            if let Err(error) = self.join_export_worker().await {
                failures.push(error);
            }
            let result = tokio::task::spawn_blocking(move || {
                provider.shutdown_with_timeout(deadline.saturating_duration_since(Instant::now()))
            })
            .await?;
            if let Err(error) = result {
                failures.push(error.into());
            }
            self.handles
                .lock()
                .map_err(|_| anyhow::anyhow!("Trace handle lock failed"))?
                .take();
        } else {
            self.shared.close()?;
            if let Err(error) = self.join_export_worker().await {
                failures.push(error);
            }
        }
        let losses = self.losses()?;
        if [
            losses.full,
            losses.oversized,
            losses.closed,
            losses.export_failed,
            losses.collector_rejected,
            losses.shutdown_discarded,
        ]
        .iter()
        .any(|count| *count != 0)
        {
            log::warn!("Trace delivery summary: {losses:?}");
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(drasi_lib::error::OperationFailures::new("Trace cleanup failures", failures).into())
        }
    }

    async fn join_export_worker(&self) -> Result<()> {
        let mut slot = self.worker.write().await;
        if let Some(join) = slot.as_mut() {
            let result = join.await;
            *slot = None;
            result?.map_err(|error| anyhow::anyhow!("Trace export worker failed: {error}"))?;
        }
        Ok(())
    }

    pub async fn wait_shutdown_until(&self, deadline: Instant) -> Result<()> {
        timeout_at(deadline, self.wait_for_completion())
            .await
            .context("Trace cleanup wait elapsed; physical workers remain owned")?
    }

    pub async fn wait_for_completion(&self) -> Result<()> {
        if let Some(result) = self
            .terminal
            .lock()
            .map_err(|_| anyhow::anyhow!("Trace terminal lock failed"))?
            .clone()
        {
            return result.map_err(|error| anyhow::anyhow!("Trace cleanup failed: {error}"));
        }
        let mut slot = self.cleanup.write().await;
        let result = if let Some(join) = slot.as_mut() {
            let result = match join.await {
                Ok(result) => result,
                Err(error) => Err(Arc::new(error.into())),
            };
            *slot = None;
            result
        } else {
            return Err(anyhow::anyhow!("Trace cleanup has not been requested"));
        };
        *self
            .terminal
            .lock()
            .map_err(|_| anyhow::anyhow!("Trace terminal lock failed"))? = Some(result.clone());
        result.map_err(|error| anyhow::anyhow!("Trace cleanup failed: {error}"))
    }

    pub async fn is_joined(&self) -> bool {
        self.worker.read().await.is_none()
            && self.cleanup.read().await.is_none()
            && self.handles.lock().is_ok_and(|handles| handles.is_none())
    }
}

impl Drop for TraceRuntime {
    fn drop(&mut self) {
        if let Err(error) = self
            .shared
            .request_stop(Instant::now())
            .and_then(|_| self.shared.close())
        {
            log::error!("Trace cleanup could not be requested: {error}");
        }
    }
}

fn run_exports(
    shared: Arc<Shared>,
    exporter: opentelemetry_otlp::SpanExporter,
    transport: ResolvedOtlpHttpClient,
) -> Result<()> {
    let mut tick = Instant::now() + EXPORT_DELAY;
    let mut active_seen = false;
    let mut warnings = ExportWarnings::default();
    let mut final_failure = false;
    loop {
        let (active, deadline, finished) = {
            let mut queue = shared.lock()?;
            loop {
                if queue.active && !active_seen {
                    active_seen = true;
                    tick = Instant::now() + EXPORT_DELAY;
                }
                let expired = queue
                    .deadline
                    .is_some_and(|deadline| Instant::now() >= deadline);
                if expired
                    || queue.native_finished
                    || (queue.active
                        && !queue.pending.is_empty()
                        && (queue.pending.len() >= BATCH_SIZE || Instant::now() >= tick))
                {
                    break;
                }
                let wake = if queue.active {
                    queue.deadline.map_or(tick, |deadline| deadline.min(tick))
                } else {
                    queue
                        .deadline
                        .unwrap_or_else(|| Instant::now() + Duration::from_secs(30))
                };
                let wait = if queue.active || queue.deadline.is_some() {
                    wake.saturating_duration_since(Instant::now())
                } else {
                    Duration::from_secs(30)
                };
                queue = shared
                    .changed
                    .wait_timeout(queue, wait)
                    .map_err(|_| anyhow::anyhow!("Trace queue lock failed"))?
                    .0;
                if Instant::now() >= tick && queue.pending.is_empty() {
                    tick = Instant::now() + EXPORT_DELAY;
                }
            }
            (queue.active, queue.deadline, queue.native_finished)
        };
        if !active || deadline.is_some_and(|deadline| Instant::now() >= deadline) {
            final_failure |= active;
            let mut queue = shared.lock()?;
            let discarded = queue.pending.len();
            let bytes: usize = queue.pending.iter().map(|pending| pending.bytes).sum();
            queue.pending.clear();
            queue.records -= discarded;
            queue.bytes -= bytes;
            queue.losses.shutdown_discarded = queue
                .losses
                .shutdown_discarded
                .saturating_add(discarded as u64);
            queue.open = false;
            shared.changed.notify_all();
            break;
        }
        let batch = shared.take_batch()?;
        if batch.is_empty() && finished {
            break;
        }
        if batch.is_empty() {
            continue;
        }
        let count = batch.len() as u64;
        struct Reservation {
            shared: Arc<Shared>,
            records: usize,
            bytes: usize,
        }
        impl Drop for Reservation {
            fn drop(&mut self) {
                match self.shared.lock() {
                    Ok(mut queue) => {
                        queue.records -= self.records;
                        queue.bytes -= self.bytes;
                        self.shared.changed.notify_all();
                    }
                    Err(error) => log::error!("Trace reservation release failed: {error}"),
                }
            }
        }
        let reservation = Reservation {
            shared: shared.clone(),
            records: batch.len(),
            bytes: batch.iter().map(|pending| pending.bytes).sum(),
        };
        let spans = batch.into_iter().map(|pending| pending.span).collect();
        let result = (|| -> Result<_> {
            transport.allow_until(deadline)?;
            tracing::subscriber::with_default(tracing::subscriber::NoSubscriber::default(), || {
                futures::executor::block_on(exporter.export(spans))
            })
            .map_err(|_| anyhow::anyhow!("OTLP trace request failed"))?;
            let response = transport.take_trace_response()?;
            anyhow::ensure!(
                response.rejected_spans <= count,
                "Invalid OTLP trace rejection count"
            );
            Ok(response)
        })();
        {
            let mut queue = shared.lock()?;
            match result {
                Ok(response) => {
                    queue.losses.exported = queue
                        .losses
                        .exported
                        .saturating_add(count - response.rejected_spans);
                    queue.losses.collector_rejected = queue
                        .losses
                        .collector_rejected
                        .saturating_add(response.rejected_spans);
                    if response.rejected_spans != 0 || response.has_warning {
                        final_failure = true;
                        if let Some(suppressed) = warnings.failed(Instant::now()) {
                            log::warn!("OTLP trace collector reported rejection/warning; {suppressed} repeated diagnostics suppressed");
                        }
                    } else if let Some(suppressed) = warnings.recovered() {
                        log::info!("OTLP trace export recovered; {suppressed} repeated diagnostics suppressed");
                    }
                }
                Err(_) => {
                    queue.losses.export_failed = queue.losses.export_failed.saturating_add(count);
                    final_failure |= deadline.is_some();
                    if let Some(suppressed) = warnings.failed(Instant::now()) {
                        log::warn!(
                            "OTLP trace export failed; {suppressed} repeated failures suppressed"
                        );
                    }
                }
            }
        }
        drop(reservation);
        tick = Instant::now() + EXPORT_DELAY;
    }
    let deadline = shared.lock()?.deadline.unwrap_or_else(Instant::now);
    exporter.shutdown_with_timeout(deadline.saturating_duration_since(Instant::now()))?;
    if final_failure {
        anyhow::bail!("Trace export completed with rejected or failed final records");
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{api::mappings::DtoMapper, config::telemetry::TelemetryConfig};
    use opentelemetry::{trace::Tracer, KeyValue};
    use opentelemetry_proto::tonic::collector::trace::v1::{
        ExportTracePartialSuccess, ExportTraceServiceRequest, ExportTraceServiceResponse,
    };
    use prost::Message;
    use wiremock::{matchers::method, Mock, MockServer, ResponseTemplate};

    fn span() -> SpanData {
        use opentelemetry::trace::{
            SpanContext, SpanId, SpanKind, TraceFlags, TraceId, TraceState,
        };
        SpanData {
            span_context: SpanContext::new(
                TraceId::from_bytes([1; 16]),
                SpanId::from_bytes([2; 8]),
                TraceFlags::SAMPLED,
                false,
                TraceState::default(),
            ),
            parent_span_id: SpanId::INVALID,
            parent_span_is_remote: false,
            span_kind: SpanKind::Internal,
            name: "test".into(),
            start_time: std::time::UNIX_EPOCH,
            end_time: std::time::UNIX_EPOCH,
            attributes: Vec::new(),
            dropped_attributes_count: 0,
            events: Default::default(),
            links: Default::default(),
            status: opentelemetry::trace::Status::Unset,
            instrumentation_scope: opentelemetry::InstrumentationScope::builder("tests").build(),
        }
    }

    async fn prepared(
        receiver: &MockServer,
    ) -> anyhow::Result<(Arc<TraceRuntime>, TraceCollection)> {
        prepared_ratio(receiver, 1.0).await
    }

    async fn prepared_ratio(
        receiver: &MockServer,
        ratio: f64,
    ) -> anyhow::Result<(Arc<TraceRuntime>, TraceCollection)> {
        let config: TelemetryConfig = serde_json::from_value(serde_json::json!({
            "tracing": {"endpoint": format!("{}/custom/traces", receiver.uri()),
                "protocol": "http/protobuf", "sampleRatio": ratio}
        }))?;
        let runtime = Arc::new(TraceRuntime::new());
        let collection = runtime
            .prepare(
                &config.resolve(&DtoMapper::new(), 8080, true)?,
                super::super::recorders::telemetry_resource("trace-worker-test"),
            )
            .await?;
        Ok((runtime, collection))
    }

    async fn exported(runtime: &TraceRuntime, count: u64) {
        tokio::time::timeout(Duration::from_secs(5), async {
            while runtime.losses().unwrap().exported < count {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await
        .unwrap();
    }

    async fn close(runtime: &Arc<TraceRuntime>) -> anyhow::Result<()> {
        let deadline = Instant::now() + Duration::from_secs(3);
        runtime.request_shutdown(deadline).await?;
        runtime.wait_shutdown_until(deadline).await
    }

    #[test]
    fn trace_admission_counts_in_flight_records_and_nested_bytes() -> anyhow::Result<()> {
        let shared = Shared::new();
        for _ in 0..2_048 {
            assert!(shared.admit(span())?);
        }
        assert!(!shared.admit(span())?);
        let batch = shared.take_batch()?;
        assert_eq!(batch.len(), 256);
        assert_eq!(shared.retained()?.0, 2_048);
        assert!(
            !shared.admit(span())?,
            "in-flight reservations must still count"
        );
        shared.release(&batch)?;
        assert!(shared.admit(span())?);

        let bytes = Shared::new();
        let mut exact = span();
        exact
            .attributes
            .push(KeyValue::new("payload", String::new()));
        let base = accounted_size(&exact).unwrap();
        exact.attributes[0] = KeyValue::new("payload", "x".repeat(16 * 1024 * 1024 - base));
        assert!(bytes.admit(exact)?);
        assert_eq!(bytes.retained()?.1, 16 * 1024 * 1024);
        assert!(!bytes.admit(span())?);

        let oversized = Shared::new();
        let mut value = span();
        value
            .attributes
            .push(KeyValue::new("payload", "x".repeat(16 * 1024 * 1024)));
        assert!(!oversized.admit(value)?);
        assert_eq!(oversized.losses()?.oversized, 1);
        let mut nested = span();
        nested.events.events.push(opentelemetry::trace::Event::new(
            "event",
            std::time::UNIX_EPOCH,
            vec![KeyValue::new("nested", "x".repeat(200))],
            0,
        ));
        assert!(accounted_size(&nested).unwrap() >= accounted_size(&span()).unwrap() + 200);
        Ok(())
    }

    #[tokio::test]
    async fn trace_batching_waits_for_activation_and_releases_reservations_once(
    ) -> anyhow::Result<()> {
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&receiver)
            .await;
        let (runtime, collection) = prepared(&receiver).await?;
        for _ in 0..300 {
            drop(collection.tracer.start("operation"));
        }
        assert!(receiver.received_requests().await.unwrap().is_empty());
        runtime.activate()?;
        close(&runtime).await?;
        let requests = receiver.received_requests().await.unwrap();
        let mut total = 0;
        for request in &requests {
            let body = ExportTraceServiceRequest::decode(request.body.as_slice())?;
            let count: usize = body
                .resource_spans
                .iter()
                .flat_map(|resource| &resource.scope_spans)
                .map(|scope| scope.spans.len())
                .sum();
            assert!(count <= 256);
            total += count;
        }
        assert_eq!(total, 300);
        assert_eq!(runtime.shared.retained()?, (0, 0));
        assert!(runtime.is_joined().await);
        Ok(())
    }

    #[tokio::test]
    async fn trace_export_failure_discards_only_its_batch_and_recovers() -> anyhow::Result<()> {
        let receiver = MockServer::start().await;
        let attempts = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let calls = attempts.clone();
        Mock::given(method("POST"))
            .respond_with(move |_: &wiremock::Request| {
                ResponseTemplate::new(
                    if calls.fetch_add(1, std::sync::atomic::Ordering::SeqCst) == 0 {
                        500
                    } else {
                        200
                    },
                )
            })
            .mount(&receiver)
            .await;
        let (runtime, collection) = prepared(&receiver).await?;
        drop(collection.tracer.start("failed"));
        runtime.activate()?;
        tokio::time::timeout(Duration::from_secs(3), async {
            while runtime.losses().unwrap().export_failed == 0 {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        drop(collection.tracer.start("recovered"));
        exported(&runtime, 1).await;
        close(&runtime).await?;
        assert_eq!(runtime.losses()?.export_failed, 1);
        assert_eq!(runtime.losses()?.exported, 1);
        assert_eq!(attempts.load(std::sync::atomic::Ordering::SeqCst), 2);
        Ok(())
    }

    #[tokio::test]
    async fn trace_partial_rejection_accounts_only_rejected_spans() -> anyhow::Result<()> {
        let receiver = MockServer::start().await;
        let response = ExportTraceServiceResponse {
            partial_success: Some(ExportTracePartialSuccess {
                rejected_spans: 2,
                error_message: "secret".into(),
            }),
        }
        .encode_to_vec();
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(response))
            .mount(&receiver)
            .await;
        let (runtime, collection) = prepared(&receiver).await?;
        for _ in 0..3 {
            drop(collection.tracer.start("operation"));
        }
        runtime.activate()?;
        assert!(
            close(&runtime).await.is_err(),
            "partial rejection must remain visible"
        );
        let losses = runtime.losses()?;
        assert_eq!((losses.exported, losses.collector_rejected), (1, 2));
        assert_eq!(receiver.received_requests().await.unwrap().len(), 1);
        Ok(())
    }

    #[tokio::test]
    async fn trace_unactivated_shutdown_discards_without_request() -> anyhow::Result<()> {
        let receiver = MockServer::start().await;
        let (runtime, collection) = prepared(&receiver).await?;
        drop(collection.tracer.start("never-active"));
        close(&runtime).await?;
        assert_eq!(runtime.losses()?.shutdown_discarded, 1);
        assert!(receiver.received_requests().await.unwrap().is_empty());
        Ok(())
    }

    #[tokio::test]
    async fn trace_native_records_use_the_prepared_context_and_processor() -> anyhow::Result<()> {
        use drasi_lib::{computation::v1::*, observability::*};
        use tracing_subscriber::prelude::*;
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&receiver)
            .await;
        let (runtime, collection) = prepared(&receiver).await?;
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(collection.tracer.clone()));
        let host = tracing::subscriber::with_default(subscriber, || tracing::info_span!("host"));
        collection.context_adapter.prepare_operation(
            &host,
            &ExecutionContext::default(),
            &[],
            RootSampling::Always,
        )?;
        let parent = collection.context_adapter.span_context(&host);
        let parent = parent.trace().unwrap();
        let graph = ComputationGraph::builder("trace-test")
            .with_telemetry(
                TelemetryOptions::new(
                    TelemetrySettings::new(TelemetryProfile::Basic).with_metrics(false),
                )
                .with_instance_id("native")
                .with_native_span_collector(collection.native_spans.clone()),
            )
            .build()?;
        let scope = TelemetryScope::graph("native", "trace-test")?.component(
            ComponentId::try_new("00000000-0000-4000-8000-000000000238")?,
            ComponentRole::Service,
            ComponentSemanticKind::Service,
        );
        let binding = graph.telemetry().plugin_binding(scope, 1, "test", None)?;
        let native = CompletedSpan {
            name: "plugin.operation".into(),
            target: "test".into(),
            context: TraceContextV1::new(parent.trace_id(), [9; 8], 1, None)?,
            parent_span_id: Some(parent.span_id()),
            links: Vec::new(),
            start_time_unix_ns: 1,
            end_time_unix_ns: 2,
            status: TelemetrySpanStatus::Ok,
            attributes: Vec::new(),
            events: Vec::new(),
        };
        assert_eq!(binding.submit_span(&native), TelemetrySubmission::Accepted);
        collection
            .native_spans
            .flush(Duration::from_secs(2))
            .await?;
        drop(host);
        runtime.activate()?;
        close(&runtime).await?;
        let requests = receiver.received_requests().await.unwrap();
        let body = ExportTraceServiceRequest::decode(requests[0].body.as_slice())?;
        let spans: Vec<_> = body
            .resource_spans
            .iter()
            .flat_map(|resource| &resource.scope_spans)
            .flat_map(|scope| &scope.spans)
            .collect();
        assert_eq!(spans.len(), 2);
        let recorded = spans
            .iter()
            .find(|span| span.name == "plugin.operation")
            .unwrap();
        assert_eq!(recorded.span_id, [9; 8]);
        assert_eq!(recorded.parent_span_id, parent.span_id());
        assert_eq!(recorded.trace_id, parent.trace_id());
        assert_eq!(
            (recorded.start_time_unix_nano, recorded.end_time_unix_nano),
            (1, 2)
        );
        Ok(())
    }

    #[tokio::test]
    async fn trace_fractional_roots_and_parent_decisions_share_the_prepared_sampler() -> Result<()>
    {
        use drasi_lib::{computation::v1::*, observability::*};
        use opentelemetry::trace::{
            SpanContext, SpanId, TraceContextExt, TraceFlags, TraceId, TraceState,
        };
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&receiver)
            .await;
        let (runtime, collection) = prepared_ratio(&receiver, 0.5).await?;
        let graph = ComputationGraph::builder("fractional")
            .with_telemetry(
                TelemetryOptions::new(
                    TelemetrySettings::new(TelemetryProfile::Basic)
                        .with_metrics(false)
                        .with_sampling(RootSampling::Ratio(SamplingRatio::new(0.5)?)),
                )
                .with_instance_id("fractional")
                .with_native_span_collector(collection.native_spans.clone()),
            )
            .build()?;
        let binding = graph.telemetry().plugin_binding(
            TelemetryScope::graph("fractional", "fractional")?.component(
                ComponentId::try_new("00000000-0000-4000-8000-000000000238")?,
                ComponentRole::Service,
                ComponentSemanticKind::Service,
            ),
            1,
            "test",
            None,
        )?;
        for (trace_id, expected) in [([1; 16], true), ([255; 16], false)] {
            assert_eq!(
                binding
                    .sample_root(&RootSpanRequest {
                        trace_id,
                        name: "plugin.root".into(),
                        target: "test".into(),
                        attributes: Vec::new(),
                        links: Vec::new(),
                    })?
                    .sampled,
                expected
            );
        }
        for (name, flags) in [
            ("sampled-child", TraceFlags::SAMPLED),
            ("unsampled-child", TraceFlags::default()),
        ] {
            let context = opentelemetry::Context::new().with_remote_span_context(SpanContext::new(
                TraceId::from(u128::MAX),
                SpanId::from(2u64),
                flags,
                true,
                TraceState::default(),
            ));
            drop(collection.tracer.start_with_context(name, &context));
        }
        runtime.activate()?;
        close(&runtime).await?;
        let mut names = Vec::new();
        for request in receiver.received_requests().await.unwrap() {
            let body = ExportTraceServiceRequest::decode(request.body.as_slice())?;
            names.extend(
                body.resource_spans
                    .into_iter()
                    .flat_map(|resource| resource.scope_spans)
                    .flat_map(|scope| scope.spans)
                    .map(|span| span.name),
            );
        }
        names.sort();
        assert_eq!(names, ["sampled-child"]);
        Ok(())
    }

    #[tokio::test]
    async fn trace_mixed_batches_preserve_128_links_and_truncation() -> Result<()> {
        use drasi_lib::observability::{
            BatchTraceParent, ExecutionContext, RootSampling, TraceContextV1, TraceLinkLimits,
        };
        use tracing_subscriber::prelude::*;
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&receiver)
            .await;
        let (runtime, collection) = prepared(&receiver).await?;
        let contexts: Vec<_> = (1..=129u8)
            .map(|id| {
                ExecutionContext::new(TraceContextV1::new([id; 16], [id; 8], 1, None).unwrap())
            })
            .collect();
        assert_eq!(
            BatchTraceParent::select(contexts.iter(), Default::default())
                .links
                .len(),
            62
        );
        let selected = BatchTraceParent::select(
            contexts.iter(),
            TraceLinkLimits {
                max_links: 128,
                max_bytes: 256 * 1024,
            },
        );
        let subscriber = tracing_subscriber::registry()
            .with(tracing_opentelemetry::layer().with_tracer(collection.tracer.clone()));
        let span = tracing::subscriber::with_default(subscriber, || {
            tracing::info_span!(target: "drasi::telemetry", "sink.handle_batch",
                links_truncated = selected.links_truncated)
        });
        collection.context_adapter.prepare_operation(
            &span,
            &selected.parent,
            &selected.links,
            RootSampling::Always,
        )?;
        drop(span);
        runtime.activate()?;
        close(&runtime).await?;
        let requests = receiver.received_requests().await.unwrap();
        let body = ExportTraceServiceRequest::decode(requests[0].body.as_slice())?;
        let span = &body.resource_spans[0].scope_spans[0].spans[0];
        assert_eq!(span.links.len(), 128);
        assert_eq!(span.dropped_links_count, 0);
        assert!(span.parent_span_id.iter().all(|byte| *byte == 0));
        assert_eq!(span.links[0].trace_id, [1; 16]);
        assert_eq!(span.links[127].trace_id, [128; 16]);
        assert!(span
            .attributes
            .iter()
            .any(|attribute| attribute.key == "links_truncated"
                && attribute
                    .value
                    .as_ref()
                    .and_then(|value| value.value.as_ref())
                    == Some(
                        &opentelemetry_proto::tonic::common::v1::any_value::Value::BoolValue(true)
                    )));
        Ok(())
    }

    #[tokio::test]
    async fn metrics_stall_does_not_delay_trace_stop_request() -> Result<()> {
        use crate::server::ServerCleanupHandle;
        let receiver = MockServer::start().await;
        let (runtime, _) = prepared(&receiver).await?;
        let owner = ServerCleanupHandle::bootstrap();
        owner.retain_traces(runtime.clone())?;
        let metrics = Arc::new(tokio::sync::Mutex::new(
            super::super::MetricsRuntime::default(),
        ));
        owner.retain_metrics(metrics.clone())?;
        let held = metrics.lock().await;
        owner.request_shutdown()?;
        tokio::time::timeout(Duration::from_secs(2), async {
            while runtime.shared.lock().unwrap().open {
                tokio::task::yield_now().await;
            }
        })
        .await?;
        assert!(runtime.shared.lock()?.deadline.is_some());
        drop(held);
        owner
            .wait_until(Instant::now() + Duration::from_secs(5))
            .await?;
        assert!(runtime.is_joined().await);
        Ok(())
    }

    async fn retained_wait() -> anyhow::Result<()> {
        let receiver = MockServer::start().await;
        Mock::given(method("POST"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(300)))
            .mount(&receiver)
            .await;
        let (runtime, collection) = prepared(&receiver).await?;
        for _ in 0..256 {
            drop(collection.tracer.start("in-flight"));
        }
        runtime.activate()?;
        tokio::time::timeout(Duration::from_secs(3), async {
            while receiver.received_requests().await.unwrap().is_empty() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
        })
        .await?;
        let deadline = Instant::now() + Duration::from_millis(20);
        runtime.request_shutdown(deadline).await?;
        let mut cancelled_wait = Box::pin(runtime.wait_for_completion());
        assert!(futures::poll!(&mut cancelled_wait).is_pending());
        runtime
            .request_shutdown(deadline + Duration::from_secs(10))
            .await?;
        drop(cancelled_wait);
        assert!(runtime.wait_shutdown_until(deadline).await.is_err());
        assert!(!runtime.is_joined().await);
        let _ = runtime.wait_for_completion().await;
        assert!(runtime.is_joined().await);
        assert_eq!(runtime.shared.retained()?, (0, 0));
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn trace_shutdown_deadline_and_cancelled_wait_retain_physical_worker(
    ) -> anyhow::Result<()> {
        retained_wait().await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn trace_shutdown_deadline_retains_physical_worker_multithread() -> anyhow::Result<()> {
        retained_wait().await
    }
}
