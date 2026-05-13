//! Minimal stub of upstream Codex's `codex-otel` crate.
//!
//! zerobox doesn't ship Codex's Statsig telemetry or OpenTelemetry exporters.
//! The upstream `windows-sandbox-rs` crate calls into `codex-otel` to record
//! Statsig metrics from the WFP setup helper. This stub provides type-compatible
//! placeholders so the upstream code compiles and links; every metric / span /
//! exporter operation becomes a no-op.

use std::collections::BTreeMap;
use std::error::Error;
use std::path::PathBuf;

use serde::Deserialize;
use serde::Serialize;

#[derive(Clone, Debug)]
pub struct OtelSettings {
    pub environment: String,
    pub service_name: String,
    pub service_version: String,
    pub codex_home: PathBuf,
    pub exporter: OtelExporter,
    pub trace_exporter: OtelExporter,
    pub metrics_exporter: OtelExporter,
    pub runtime_metrics: bool,
    pub span_attributes: BTreeMap<String, String>,
    pub tracestate: BTreeMap<String, BTreeMap<String, String>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatsigMetricsSettings {
    pub environment: String,
}

#[derive(Clone, Debug)]
pub enum OtelExporter {
    None,
    Statsig,
}

pub struct OtelProvider {}

impl OtelProvider {
    pub fn from(_settings: &OtelSettings) -> Result<Option<Self>, Box<dyn Error>> {
        Ok(None)
    }

    pub fn metrics(&self) -> Option<&MetricsClient> {
        None
    }

    pub fn shutdown(&self) {}
}

pub struct MetricsClient {}

impl MetricsClient {
    pub fn counter(
        &self,
        _name: &str,
        _inc: u64,
        _tags: &[(&str, &str)],
    ) -> Result<(), Box<dyn Error>> {
        Ok(())
    }

    pub fn shutdown(&self) {}
}

pub fn global_statsig_metrics_settings() -> Option<StatsigMetricsSettings> {
    None
}
