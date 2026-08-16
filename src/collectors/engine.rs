use anyhow::Result;
use chrono::Utc;
use std::sync::Arc;
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::db::sqlite::repository::{
    MetricRepository, MonitorRepository, ResourceRepository, TelemetryRepository,
};
use crate::domain::metric::MetricValueType;
use crate::domain::monitor::ConfigState;
use crate::domain::resource::ResourceState;
use crate::domain::telemetry::{ObservationState, Telemetry};

use crate::collectors::service::LinuxServiceCollector;
use crate::collectors::traits::Collector;

pub struct TelemetryEngine<'a> {
    monitor_repo: &'a MonitorRepository<'a>,
    resource_repo: &'a ResourceRepository<'a>,
    telemetry_repo: &'a TelemetryRepository<'a>,
    metric_repo: &'a MetricRepository<'a>,
    service_collector: LinuxServiceCollector,
}

impl<'a> TelemetryEngine<'a> {
    pub fn new(
        monitor_repo: &'a MonitorRepository<'a>,
        resource_repo: &'a ResourceRepository<'a>,
        telemetry_repo: &'a TelemetryRepository<'a>,
        metric_repo: &'a MetricRepository<'a>,
        service_collector: LinuxServiceCollector,
    ) -> Self {
        Self {
            monitor_repo,
            resource_repo,
            telemetry_repo,
            metric_repo,
            service_collector,
        }
    }

    /// Run a single tick of the collection loop.
    /// In a real system, this would filter by `Monitor.interval_secs` and `last_collected_at`,
    /// but for the prototype we iterate over all Monitored resources to process them.
    pub async fn run_collection_tick(
        &self,
        node: &str,
        proxmox_vmid: u32,
        vm_id: Uuid,
    ) -> Result<()> {
        debug!("Running collection tick for VM {}", vm_id);

        // We only care about resources that exist for this VM
        // Note: For abstraction we just fetch 'service' here. Future: iterate over all kinds.
        let resources = self.resource_repo.list_by_vm_and_kind(vm_id, "service")?;

        for resource in resources {
            // ONLY collect if ResourceState == Monitored
            if resource.state != ResourceState::Monitored {
                debug!(
                    "Skipping collection for {}/{}: State is {:?}",
                    resource.kind, resource.identifier, resource.state
                );
                continue;
            }

            // Find an ENABLED monitor for this resource
            let monitors = self.monitor_repo.get_by_resource_id(resource.id)?;
            let active_monitor = monitors
                .into_iter()
                .find(|m| m.state == ConfigState::Enabled);

            if let Some(monitor) = active_monitor {
                // Determine collector based on kind or collection_type
                // For this implementation, we hardcode the mapping.
                if monitor.collection_type == "linux_service" || resource.kind == "service" {
                    self.collect_service(&resource, &monitor, node, proxmox_vmid)
                        .await?;
                }
            } else {
                debug!(
                    "Skipping collection for {}/{}: No ENABLED monitor found",
                    resource.kind, resource.identifier
                );
            }
        }

        Ok(())
    }

    async fn collect_service(
        &self,
        resource: &crate::domain::resource::Resource,
        monitor: &crate::domain::monitor::Monitor,
        node: &str,
        proxmox_vmid: u32,
    ) -> Result<()> {
        let result = self
            .service_collector
            .collect(node, proxmox_vmid, &resource.identifier)
            .await?;

        // Find the matching Metric definition (e.g. "service_status")
        let metric = self
            .metric_repo
            .get_by_monitor_and_name(monitor.id, "service_status")?;

        if let Some(metric) = metric {
            let mut val_num = result.value_numeric;
            let mut val_str = result.value_string;

            // Map types safely based on metric.value_type
            if metric.value_type == MetricValueType::State {
                // For state types, we want the string representation
                // `systemctl is-active` returns strings like "active" or "inactive"
                val_num = None;
            }

            let t = Telemetry {
                id: Uuid::new_v4(),
                metric_id: metric.id,
                timestamp: Utc::now(),
                value: val_num,
                string_value: val_str,
                observation: result.observation,
                labels: serde_json::json!({
                    "vmid": proxmox_vmid.to_string(),
                    "service": resource.identifier
                }),
            };

            self.telemetry_repo.insert(&t)?;
            debug!(
                "Collected telemetry for {}: {:?}",
                resource.identifier, t.observation
            );
        } else {
            warn!(
                "No 'service_status' metric defined for monitor {}",
                monitor.id
            );
        }

        Ok(())
    }
}
