// Copyright 2026 The Drasi Authors.
// Licensed under the Apache License, Version 2.0.

//! Whole-process resource measurements projected from one raw
//! `metrics-process` snapshot.
//!
//! Only the raw collector is used: Server installs no process-global `metrics`
//! recorder, and the stock collector truncates CPU time and hides failures.
//! Unavailable, failed, non-finite and unlimited readings are omitted, never
//! published as `0` or a stale value.

use drasi_lib::observability::ResolvedTelemetry;
use metrics_process::collector::{collect, Metrics};
use std::{
    sync::{Arc, Mutex, PoisonError},
    time::{Duration, Instant},
};

pub(crate) const MISSING_WARNING_INTERVAL: Duration = Duration::from_secs(300);

/// Linux/FreeBSD report an unlimited soft limit as `0`, macOS as the raw
/// `RLIM_INFINITY`.
const UNLIMITED_FDS: u64 = 0x7fff_ffff_ffff_ffff;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProcessMeasurement {
    CpuSecondsTotal,
    ResidentMemoryBytes,
    VirtualMemoryBytes,
    OpenFds,
    MaxFds,
    Threads,
    StartTimeSeconds,
}

impl ProcessMeasurement {
    pub(crate) const ALL: [ProcessMeasurement; 7] = [
        Self::CpuSecondsTotal,
        Self::ResidentMemoryBytes,
        Self::VirtualMemoryBytes,
        Self::OpenFds,
        Self::MaxFds,
        Self::Threads,
        Self::StartTimeSeconds,
    ];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::CpuSecondsTotal => "process_cpu_seconds_total",
            Self::ResidentMemoryBytes => "process_resident_memory_bytes",
            Self::VirtualMemoryBytes => "process_virtual_memory_bytes",
            Self::OpenFds => "process_open_fds",
            Self::MaxFds => "process_max_fds",
            Self::Threads => "process_threads",
            Self::StartTimeSeconds => "process_start_time_seconds",
        }
    }

    pub(crate) fn help(self) -> &'static str {
        match self {
            Self::CpuSecondsTotal => "Total user and system CPU time spent in seconds.",
            Self::ResidentMemoryBytes => "Resident memory size in bytes.",
            Self::VirtualMemoryBytes => "Virtual memory size in bytes.",
            Self::OpenFds => "Number of open file descriptors.",
            Self::MaxFds => "Maximum number of open file descriptors.",
            Self::Threads => "Number of OS threads in the process.",
            Self::StartTimeSeconds => "Start time of the process since unix epoch in seconds.",
        }
    }

    pub(crate) fn is_counter(self) -> bool {
        self == Self::CpuSecondsTotal
    }

    pub(crate) fn otlp_unit(self) -> &'static str {
        match self {
            Self::CpuSecondsTotal | Self::StartTimeSeconds => "s",
            Self::ResidentMemoryBytes | Self::VirtualMemoryBytes => "By",
            Self::OpenFds | Self::MaxFds | Self::Threads => "1",
        }
    }

    /// Whether `metrics-process` 2.4.3 provides this reading on the current OS.
    pub(crate) fn expected_on_this_platform(self) -> bool {
        match self {
            Self::Threads => cfg!(any(
                target_os = "linux",
                target_os = "macos",
                target_os = "freebsd"
            )),
            _ => cfg!(any(
                target_os = "linux",
                target_os = "macos",
                target_os = "windows",
                target_os = "freebsd"
            )),
        }
    }

    fn read(self, snapshot: &Metrics) -> Option<f64> {
        let whole = |reading: Option<u64>| reading.map(|value| value as f64);
        match self {
            Self::CpuSecondsTotal => snapshot.cpu_seconds_total,
            Self::ResidentMemoryBytes => whole(snapshot.resident_memory_bytes),
            Self::VirtualMemoryBytes => whole(snapshot.virtual_memory_bytes),
            Self::OpenFds => whole(snapshot.open_fds),
            Self::MaxFds => whole(snapshot.max_fds),
            Self::Threads => whole(snapshot.threads),
            Self::StartTimeSeconds => whole(snapshot.start_time_seconds),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct ProcessSample {
    pub(crate) measurement: ProcessMeasurement,
    pub(crate) value: f64,
}

pub(crate) struct Projection {
    pub(crate) samples: Vec<ProcessSample>,
    /// Enabled measurements this platform should provide but did not.
    pub(crate) missing: Vec<ProcessMeasurement>,
}

pub(crate) fn project(snapshot: &Metrics, enabled: &[ProcessMeasurement]) -> Projection {
    let mut projection = Projection {
        samples: Vec::with_capacity(enabled.len()),
        missing: Vec::new(),
    };
    for &measurement in enabled {
        if measurement == ProcessMeasurement::MaxFds
            && snapshot
                .max_fds
                .is_some_and(|max| max == 0 || max >= UNLIMITED_FDS)
        {
            continue;
        }
        match measurement.read(snapshot).filter(|value| value.is_finite()) {
            Some(value) => projection
                .samples
                .push(ProcessSample { measurement, value }),
            None if measurement.expected_on_this_platform() => projection.missing.push(measurement),
            None => {}
        }
    }
    projection
}

pub(crate) type SnapshotSource = Arc<dyn Fn() -> Metrics + Send + Sync>;

/// Samples the enabled process measurements; callers run it on owned blocking
/// workers only.
pub(crate) struct ProcessMetrics {
    enabled: Vec<ProcessMeasurement>,
    source: SnapshotSource,
    warnings: Mutex<MissingWarnings>,
}

impl ProcessMetrics {
    pub(crate) fn new(policy: &ResolvedTelemetry) -> Arc<Self> {
        Self::with_source(policy, Arc::new(collect))
    }

    pub(crate) fn with_source(policy: &ResolvedTelemetry, source: SnapshotSource) -> Arc<Self> {
        let enabled = ProcessMeasurement::ALL
            .into_iter()
            .filter(|measurement| policy.allows_metric(measurement.name()))
            .collect();
        Arc::new(Self {
            enabled,
            source,
            warnings: Mutex::default(),
        })
    }

    /// Takes one snapshot; a failed reading never fails the caller.
    pub(crate) fn sample(&self) -> Vec<ProcessSample> {
        let projection = project(&(self.source)(), &self.enabled);
        let decision = self
            .warnings
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .observe(!projection.missing.is_empty(), Instant::now());
        match decision {
            WarningDecision::Warn { suppressed } => {
                let names = projection
                    .missing
                    .iter()
                    .map(|measurement| measurement.name())
                    .collect::<Vec<_>>()
                    .join(", ");
                log::warn!("Process metrics unavailable this cycle: {names}; {suppressed} repeated warnings suppressed");
            }
            WarningDecision::Recovered { suppressed } => {
                log::info!("Process metrics recovered; {suppressed} repeated warnings suppressed");
            }
            WarningDecision::Quiet | WarningDecision::Suppressed => {}
        }
        projection.samples
    }

    /// Appends one snapshot as unlabeled Prometheus text families.
    pub(crate) fn append_prometheus(&self, text: &mut String) {
        use std::fmt::Write;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        for ProcessSample { measurement, value } in self.sample() {
            let name = measurement.name();
            let help = measurement.help();
            let kind = if measurement.is_counter() {
                "counter"
            } else {
                "gauge"
            };
            // Writing to a `String` cannot fail.
            let _ = writeln!(
                text,
                "# HELP {name} {help}\n# TYPE {name} {kind}\n{name} {value}"
            );
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum WarningDecision {
    Quiet,
    Warn { suppressed: u64 },
    Suppressed,
    Recovered { suppressed: u64 },
}

/// Rate-limits missing-measurement warnings like `ExportWarnings`, with a
/// longer window because scrapes can be frequent.
#[derive(Default)]
pub(crate) struct MissingWarnings {
    last_warning: Option<Instant>,
    suppressed: u64,
    failing: bool,
}

impl MissingWarnings {
    pub(crate) fn observe(&mut self, missing_any: bool, now: Instant) -> WarningDecision {
        if !missing_any {
            return if std::mem::take(&mut self.failing) {
                WarningDecision::Recovered {
                    suppressed: std::mem::take(&mut self.suppressed),
                }
            } else {
                WarningDecision::Quiet
            };
        }
        self.failing = true;
        if self
            .last_warning
            .is_some_and(|last| now.duration_since(last) < MISSING_WARNING_INTERVAL)
        {
            self.suppressed = self.suppressed.saturating_add(1);
            return WarningDecision::Suppressed;
        }
        self.last_warning = Some(now);
        WarningDecision::Warn {
            suppressed: std::mem::take(&mut self.suppressed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use drasi_lib::observability::{TelemetryProfile, TelemetrySettings};
    use ProcessMeasurement::*;

    fn policy(exclude: &[&str]) -> ResolvedTelemetry {
        exclude
            .iter()
            .fold(
                TelemetrySettings::new(TelemetryProfile::Basic).with_metrics(true),
                |settings, part| settings.exclude_metrics(*part),
            )
            .resolve()
            .unwrap()
    }

    fn sample(measurement: ProcessMeasurement, value: f64) -> ProcessSample {
        ProcessSample { measurement, value }
    }

    #[test]
    fn projection_omits_missing_unlimited_and_non_finite_values() {
        let snapshot = Metrics {
            cpu_seconds_total: Some(f64::NAN),
            resident_memory_bytes: Some(4096),
            virtual_memory_bytes: None,
            open_fds: Some(12),
            max_fds: Some(0),
            threads: Some(9),
            start_time_seconds: Some(1_700_000_000),
            virtual_memory_max_bytes: Some(123),
        };
        let projection = project(&snapshot, &ProcessMeasurement::ALL);
        assert_eq!(
            projection.samples,
            vec![
                sample(ResidentMemoryBytes, 4096.0),
                sample(OpenFds, 12.0),
                sample(Threads, 9.0),
                sample(StartTimeSeconds, 1_700_000_000.0),
            ]
        );
        assert_eq!(
            projection.missing,
            vec![CpuSecondsTotal, VirtualMemoryBytes]
        );
        assert!(projection.samples.iter().all(|s| s.value != 0.0));
        assert!(projection.samples.iter().all(|s| s.measurement != MaxFds));

        for max in [0, 0x7fff_ffff_ffff_ffff, u64::MAX] {
            let snapshot = Metrics {
                max_fds: Some(max),
                ..Default::default()
            };
            let projection = project(&snapshot, &[MaxFds]);
            assert!(projection.samples.is_empty(), "max_fds {max} published");
            assert!(
                projection.missing.is_empty(),
                "max_fds {max} reported missing"
            );
        }
        let snapshot = Metrics {
            max_fds: Some(10240),
            ..Default::default()
        };
        assert_eq!(
            project(&snapshot, &[MaxFds]).samples,
            vec![sample(MaxFds, 10240.0)]
        );
    }

    #[test]
    fn projection_uses_fractional_cpu_and_respects_exclusions() {
        let source: SnapshotSource = Arc::new(|| Metrics {
            cpu_seconds_total: Some(1.25),
            resident_memory_bytes: Some(1),
            virtual_memory_bytes: Some(2),
            ..Default::default()
        });
        let samples = ProcessMetrics::with_source(&policy(&["memory"]), source.clone()).sample();
        assert_eq!(samples, vec![sample(CpuSecondsTotal, 1.25)]);
        assert!(ProcessMetrics::with_source(&policy(&["process_"]), source)
            .sample()
            .is_empty());
    }

    #[test]
    fn missing_measurements_warn_once_per_interval_and_report_recovery() {
        let t0 = Instant::now();
        let s = |secs| t0 + Duration::from_secs(secs);
        let mut w = MissingWarnings::default();
        assert_eq!(
            w.observe(true, s(0)),
            WarningDecision::Warn { suppressed: 0 }
        );
        assert_eq!(w.observe(true, s(10)), WarningDecision::Suppressed);
        assert_eq!(w.observe(true, s(299)), WarningDecision::Suppressed);
        assert_eq!(
            w.observe(true, s(300)),
            WarningDecision::Warn { suppressed: 2 }
        );
        assert_eq!(
            w.observe(false, s(301)),
            WarningDecision::Recovered { suppressed: 0 }
        );
        assert_eq!(w.observe(false, s(302)), WarningDecision::Quiet);
        assert_eq!(w.observe(true, s(303)), WarningDecision::Suppressed);
        assert_eq!(
            w.observe(true, s(600)),
            WarningDecision::Warn { suppressed: 1 }
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn real_sample_reports_platform_measurements() {
        let deadline = Instant::now() + Duration::from_millis(50);
        let mut spins = 0_u64;
        while Instant::now() < deadline {
            spins = std::hint::black_box(spins.wrapping_add(1));
        }
        let samples = ProcessMetrics::new(&policy(&[])).sample();
        for measurement in ProcessMeasurement::ALL {
            if measurement != MaxFds && measurement.expected_on_this_platform() {
                assert!(
                    samples.iter().any(|s| s.measurement == measurement),
                    "{measurement:?} missing from {samples:?}"
                );
            }
        }
        let cpu = samples
            .iter()
            .find(|s| s.measurement == CpuSecondsTotal)
            .unwrap()
            .value;
        assert!(cpu.is_finite() && cpu > 0.0, "cpu {cpu}");
    }
}
