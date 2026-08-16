use anyhow::Result;
use std::sync::{Arc, RwLock};
use tracing::{debug, warn};

use crate::db::sqlite::repository::{ExporterQueries, TelemetryRepository};
use crate::domain::telemetry::ObservationState;

pub struct PrometheusExporter<'a> {
    exporter_queries: &'a ExporterQueries<'a>,
    telemetry_repo: &'a TelemetryRepository<'a>,
    snapshot: Arc<RwLock<String>>,
}

impl<'a> PrometheusExporter<'a> {
    pub fn new(
        exporter_queries: &'a ExporterQueries<'a>,
        telemetry_repo: &'a TelemetryRepository<'a>,
    ) -> Self {
        Self {
            exporter_queries,
            telemetry_repo,
            snapshot: Arc::new(RwLock::new(String::new())),
        }
    }

    pub fn get_snapshot_ref(&self) -> Arc<RwLock<String>> {
        Arc::clone(&self.snapshot)
    }

    /// Iterates over the SQLite source of truth and rebuilds the in-memory Prometheus snapshot.
    /// Does NOT perform any rule evaluation or retention cleanup. Purely read-only.
    pub fn update_snapshot(&self) -> Result<()> {
        debug!("Updating Prometheus metrics snapshot...");

        let metrics = self.exporter_queries.get_monitored_metrics_with_vm()?;

        let mut buffer = String::with_capacity(1024 * 64);
        buffer.push_str("# HELP sentinel_exporter_last_update Timestamp of the last successful snapshot update\n");
        buffer.push_str("# TYPE sentinel_exporter_last_update gauge\n");
        buffer.push_str(&format!(
            "sentinel_exporter_last_update {}\n\n",
            chrono::Utc::now().timestamp()
        ));

        for (metric_id, proxmox_vmid, vm_name, resource_kind, resource_identifier, metric_name) in
            metrics
        {
            let latest_telemetry = match self.telemetry_repo.get_latest_for_metric(metric_id) {
                Ok(Some(t)) => t,
                Ok(None) => continue, // No data for this metric yet
                Err(e) => {
                    warn!("Failed to fetch telemetry for metric {}: {}", metric_id, e);
                    continue;
                }
            };

            let safe_kind = resource_kind.replace("-", "_").to_lowercase();
            let safe_metric = metric_name.replace(" ", "_").to_lowercase();
            let prom_metric_name = format!("sentinel_{}_{}", safe_kind, safe_metric);

            let state_val = match latest_telemetry.observation {
                ObservationState::Healthy => 1,
                ObservationState::Problem => 0,
                ObservationState::Unknown => -1,
            };

            let labels = format!(
                "vm=\"{}\",vm_name=\"{}\",resource=\"{}\"",
                proxmox_vmid, vm_name, resource_identifier
            );

            buffer.push_str(&format!(
                "{}_observation_state{{{}}} {}\n",
                prom_metric_name, labels, state_val
            ));

            if latest_telemetry.observation != ObservationState::Unknown {
                if let Some(num_val) = latest_telemetry.value {
                    buffer.push_str(&format!("{}{{{}}} {}\n", prom_metric_name, labels, num_val));
                } else if let Some(str_val) = latest_telemetry.string_value {
                    buffer.push_str(&format!(
                        "{}_info{{{},value=\"{}\"}} 1\n",
                        prom_metric_name, labels, str_val
                    ));
                }
            }

            buffer.push('\n');
        }

        let mut lock = self.snapshot.write().unwrap();
        *lock = buffer;

        debug!("Prometheus snapshot updated successfully.");
        Ok(())
    }
}
