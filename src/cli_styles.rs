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

//! CLI styling helpers for plugin commands.
//!
//! Provides colored output, unicode status symbols, and spinners
//! for a polished terminal experience.

use console::Style;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;

// ── Status symbols ──────────────────────────────────────────────

/// Green checkmark for success.
pub fn success(msg: &str) -> String {
    format!("{} {}", Style::new().green().bold().apply_to("✓"), msg)
}

/// Red cross for errors.
pub fn error(msg: &str) -> String {
    format!("{} {}", Style::new().red().bold().apply_to("✗"), msg)
}

/// Cyan down-arrow for downloads.
pub fn download(msg: &str) -> String {
    format!("{} {}", Style::new().cyan().bold().apply_to("↓"), msg)
}

/// Yellow symbol for skipped items.
pub fn skip(msg: &str) -> String {
    format!("{} {}", Style::new().yellow().apply_to("⊘"), msg)
}

/// Dimmed detail text, indented.
pub fn detail(msg: &str) -> String {
    format!("  {}", Style::new().dim().apply_to(msg))
}

// ── Formatting helpers ──────────────────────────────────────────

/// Bold heading text.
pub fn heading(msg: &str) -> String {
    Style::new().bold().apply_to(msg).to_string()
}

/// Format a version string with green color.
pub fn version(v: &str) -> String {
    Style::new().green().apply_to(v).to_string()
}

/// Format a version upgrade: old → new.
pub fn version_upgrade(old: &str, new: &str) -> String {
    format!(
        "{} → {}",
        Style::new().dim().apply_to(old),
        Style::new().green().bold().apply_to(new)
    )
}

/// Format a file path in dim text.
pub fn path(p: &str) -> String {
    Style::new().dim().apply_to(p).to_string()
}

/// Print a section header with a horizontal rule.
pub fn section(title: &str) {
    let rule = "─".repeat(40);
    println!(
        "\n── {} {}",
        Style::new().bold().apply_to(title),
        Style::new().dim().apply_to(rule)
    );
}

/// Print a summary line with counts.
pub fn summary(upgraded: usize, up_to_date: usize, skipped: usize, failed: usize) {
    let mut parts = Vec::new();
    if upgraded > 0 {
        parts.push(format!(
            "{} {}",
            Style::new().green().bold().apply_to(upgraded),
            Style::new().green().apply_to("upgraded")
        ));
    }
    if up_to_date > 0 {
        parts.push(format!(
            "{} up to date",
            Style::new().bold().apply_to(up_to_date)
        ));
    }
    if skipped > 0 {
        parts.push(format!(
            "{} {}",
            Style::new().yellow().bold().apply_to(skipped),
            Style::new().yellow().apply_to("skipped")
        ));
    }
    if failed > 0 {
        parts.push(format!(
            "{} {}",
            Style::new().red().bold().apply_to(failed),
            Style::new().red().apply_to("failed")
        ));
    }
    println!("\n  {}", parts.join(", "));
}

/// Print an install/download summary with counts.
pub fn install_summary(installed: usize, existing: usize, failed: usize) {
    let mut parts = Vec::new();
    if installed > 0 {
        parts.push(format!(
            "{} {}",
            Style::new().green().bold().apply_to(installed),
            Style::new().green().apply_to("installed")
        ));
    }
    if existing > 0 {
        parts.push(format!(
            "{} already installed",
            Style::new().bold().apply_to(existing)
        ));
    }
    if failed > 0 {
        parts.push(format!(
            "{} {}",
            Style::new().red().bold().apply_to(failed),
            Style::new().red().apply_to("failed")
        ));
    }
    println!("\n  {}", parts.join(", "));
}

/// Green "trusted ✓" signature label — signed and matches a trusted identity.
pub fn sig_trusted(issuer: &str, subject: &str) -> String {
    format!(
        "{}\n      Issuer:  {}\n      Subject: {}",
        Style::new().green().bold().apply_to("trusted ✓"),
        Style::new().dim().apply_to(issuer),
        Style::new().dim().apply_to(subject),
    )
}

/// Yellow "signed ✓" label — signed but signer not in trusted identities.
pub fn sig_signed_untrusted(issuer: &str, subject: &str) -> String {
    format!(
        "{}\n      Issuer:  {}\n      Subject: {}",
        Style::new().yellow().bold().apply_to("signed ✓"),
        Style::new().dim().apply_to(issuer),
        Style::new().dim().apply_to(subject),
    )
}

/// Red "unsigned" signature label (using 256-color for a softer red).
pub fn sig_unsigned() -> String {
    Style::new()
        .color256(203)
        .apply_to("unsigned ✗")
        .to_string()
}

/// Yellow "unverified" signature label.
pub fn sig_unverified() -> String {
    Style::new().yellow().apply_to("unverified").to_string()
}

/// Bright red "TAMPERED" signature label — signature exists but verification failed.
pub fn sig_tampered(reason: &str) -> String {
    format!(
        "{}  ({})",
        Style::new().red().bold().apply_to("TAMPERED ⚠"),
        Style::new().red().dim().apply_to(reason),
    )
}

/// Format file integrity status for display.
pub fn integrity_status(status: &drasi_server::plugin_lockfile::FileIntegrityStatus) -> String {
    use drasi_server::plugin_lockfile::FileIntegrityStatus;
    match status {
        FileIntegrityStatus::Ok => Style::new()
            .green()
            .apply_to("integrity: verified ✓")
            .to_string(),
        FileIntegrityStatus::Tampered { .. } => Style::new()
            .red()
            .bold()
            .apply_to("integrity: TAMPERED ⚠")
            .to_string(),
        FileIntegrityStatus::NoHash => Style::new()
            .dim()
            .apply_to("integrity: no hash recorded")
            .to_string(),
        FileIntegrityStatus::Missing => Style::new()
            .yellow()
            .apply_to("integrity: file missing")
            .to_string(),
        FileIntegrityStatus::Error(e) => format!(
            "{} ({})",
            Style::new().yellow().apply_to("integrity: error"),
            Style::new().dim().apply_to(e),
        ),
    }
}

/// Determine the signature display string based on lockfile info and trusted identities.
///
/// - Signed + matches trusted identity → green "trusted ✓"
/// - Signed + no trusted identity match → yellow "signed ✓"
/// - Not signed → salmon "unsigned ✗"
pub fn sig_status(
    sig: Option<&drasi_server::plugin_lockfile::PluginSignatureInfo>,
    trusted: &[drasi_host_sdk::registry::TrustedIdentity],
) -> String {
    match sig {
        Some(s) if s.verified => {
            let vr = drasi_host_sdk::registry::VerificationResult {
                issuer: s.issuer.clone(),
                subject: s.subject.clone(),
            };
            if drasi_host_sdk::registry::matches_trusted_identity(&vr, trusted) {
                sig_trusted(&s.issuer, &s.subject)
            } else {
                sig_signed_untrusted(&s.issuer, &s.subject)
            }
        }
        Some(_) => sig_unverified(),
        None => sig_unsigned(),
    }
}

/// Determine the signature display string from a `SignatureStatus`.
pub fn sig_status_from_result(
    status: &drasi_host_sdk::registry::SignatureStatus,
    trusted: &[drasi_host_sdk::registry::TrustedIdentity],
) -> String {
    match status {
        drasi_host_sdk::registry::SignatureStatus::Verified(v) => {
            if drasi_host_sdk::registry::matches_trusted_identity(v, trusted) {
                sig_trusted(&v.issuer, &v.subject)
            } else {
                sig_signed_untrusted(&v.issuer, &v.subject)
            }
        }
        drasi_host_sdk::registry::SignatureStatus::Tampered(reason) => sig_tampered(reason),
        drasi_host_sdk::registry::SignatureStatus::Unsigned => sig_unsigned(),
    }
}

// ── Spinners ────────────────────────────────────────────────────

/// Create a spinner with a message. Call `.finish_and_clear()` or
/// `.finish_with_message()` when the operation completes.
pub fn spinner(msg: &str) -> ProgressBar {
    let pb = ProgressBar::new_spinner();
    pb.set_style(
        ProgressStyle::with_template("{spinner:.cyan} {msg}")
            .expect("invalid progress template")
            .tick_strings(&["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"]),
    );
    pb.set_message(msg.to_string());
    pb.enable_steady_tick(Duration::from_millis(80));
    pb
}
