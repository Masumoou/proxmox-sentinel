use anyhow::Result;
use async_trait::async_trait;
use uuid::Uuid;

use crate::domain::telemetry::ObservationState;

/// Result returned from a specific collector
pub struct CollectorResult {
    /// E.g. "running", "stopped", or numeric string
    pub value_string: Option<String>,
    pub value_numeric: Option<f64>,
    pub observation: ObservationState,
}

#[async_trait]
pub trait Collector: Send + Sync {
    /// Given a node, proxmox_vmid, and the resource identifier, collect the state.
    async fn collect(
        &self,
        node: &str,
        proxmox_vmid: u32,
        identifier: &str,
    ) -> Result<CollectorResult>;
}
