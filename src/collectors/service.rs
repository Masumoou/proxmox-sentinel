use anyhow::Result;
use async_trait::async_trait;
use std::sync::Arc;
use tracing::warn;

use crate::collectors::traits::{Collector, CollectorResult};
use crate::domain::telemetry::ObservationState;
use crate::proxmox_api::ProxmoxClient;

pub struct LinuxServiceCollector {
    pve_client: Arc<ProxmoxClient>,
}

impl LinuxServiceCollector {
    pub fn new(pve_client: Arc<ProxmoxClient>) -> Self {
        Self { pve_client }
    }
}

#[async_trait]
impl Collector for LinuxServiceCollector {
    async fn collect(
        &self,
        node: &str,
        proxmox_vmid: u32,
        identifier: &str,
    ) -> Result<CollectorResult> {
        let command = format!("systemctl is-active {}", identifier);

        match self
            .pve_client
            .vm_agent_exec_shell(node, proxmox_vmid, &command)
            .await
        {
            Ok(output) => {
                let status = output.trim().to_string(); // 'active', 'inactive', 'failed', etc.
                Ok(CollectorResult {
                    value_string: Some(status),
                    value_numeric: None,
                    observation: ObservationState::Healthy, // We successfully got the state!
                })
            }
            Err(e) => {
                // Determine if it's a guest agent offline issue vs command failure
                warn!(
                    "QEMU Guest agent execution failed on VM {} for service {}: {}",
                    proxmox_vmid, identifier, e
                );
                // According to our rule: Guest Agent failure = UNKNOWN, not DOWN.
                // We return Unknown to halt the rule engine from interpreting a missed heartbeat as a service crash.
                Ok(CollectorResult {
                    value_string: None,
                    value_numeric: None,
                    observation: ObservationState::Unknown,
                })
            }
        }
    }
}
