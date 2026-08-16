use anyhow::Result;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use crate::db::sqlite::repository::{DiscoveryEventRepository, ResourceRepository};
use crate::domain::discovery::{DiscoveryEvent, DiscoveryEventType};
use crate::domain::resource::{Resource, ResourceState};
use crate::proxmox_api::{GuestKind, ProxmoxClient};

pub struct DiscoveryEngine<'a> {
    pve_client: Arc<ProxmoxClient>,
    resource_repo: &'a ResourceRepository<'a>,
    event_repo: &'a DiscoveryEventRepository<'a>,
}

impl<'a> DiscoveryEngine<'a> {
    pub fn new(
        pve_client: Arc<ProxmoxClient>,
        resource_repo: &'a ResourceRepository<'a>,
        event_repo: &'a DiscoveryEventRepository<'a>,
    ) -> Self {
        Self {
            pve_client,
            resource_repo,
            event_repo,
        }
    }

    /// Orchestrate a full discovery scan across all nodes in the cluster
    pub async fn run_full_scan(&self) -> Result<()> {
        info!("Starting cluster-wide discovery scan");
        let nodes = self.pve_client.list_nodes().await?;

        for node in nodes {
            if let Err(e) = self.scan_node(&node).await {
                error!("Failed to scan node {}: {}", node, e);
            }
        }

        info!("Finished cluster-wide discovery scan");
        Ok(())
    }

    async fn scan_node(&self, node: &str) -> Result<()> {
        info!("Scanning node: {}", node);
        let guests = self.pve_client.list_guests(node).await?;

        for guest in guests {
            if guest.status == "running" && guest.kind == GuestKind::Vm {
                // Mocking VM UUID generation for demonstration
                let vm_id = Uuid::new_v5(&Uuid::NAMESPACE_DNS, guest.vmid.to_string().as_bytes());

                if let Err(e) = self.scan_guest(vm_id, guest.vmid, node).await {
                    warn!(
                        "Failed to discover services for VM {} on {}: {}",
                        guest.vmid, node, e
                    );
                }
            }
        }

        Ok(())
    }

    async fn scan_guest(&self, vm_id: Uuid, proxmox_vmid: u32, node: &str) -> Result<()> {
        debug!(
            "Running Guest Agent discovery on VM {} (Proxmox VMID: {})",
            vm_id, proxmox_vmid
        );

        // 1. Verify Guest Agent is responsive
        if self
            .pve_client
            .vm_agent_ping(node, proxmox_vmid)
            .await
            .is_err()
        {
            warn!(
                "QEMU Guest Agent not responding on VM {}. Discovery skipped, but resources remain historically preserved.",
                proxmox_vmid
            );
            // We DO NOT mark resources as Removed here, preserving state.
            return Ok(());
        }

        // 2. Discover Linux Services (All enabled services, not just running)
        let services = self.discover_linux_services(node, proxmox_vmid).await?;

        // 3. Reconcile with Database
        self.reconcile_resources(vm_id, "service", services)?;

        Ok(())
    }

    async fn discover_linux_services(&self, node: &str, proxmox_vmid: u32) -> Result<Vec<String>> {
        // Query ALL installed services, not just running ones.
        // This allows user to monitor if a service has stopped.
        let command =
            "systemctl list-unit-files --type=service --no-pager --no-legend | awk '{print }'";

        let output = self
            .pve_client
            .vm_agent_exec_shell(node, proxmox_vmid, command)
            .await?;

        let services: Vec<String> = output
            .lines()
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .collect();

        Ok(services)
    }

    fn reconcile_resources(
        &self,
        vm_id: Uuid,
        kind: &str,
        scanned_identifiers: Vec<String>,
    ) -> Result<()> {
        // 1. Get existing resources from DB for this VM & Kind
        let existing = self.resource_repo.list_by_vm_and_kind(vm_id, kind)?;
        let mut existing_map: HashMap<String, Resource> = HashMap::new();

        for res in existing {
            existing_map.insert(res.identifier.clone(), res);
        }

        // 2. Process what we found in the scan
        for scanned_id in scanned_identifiers {
            if let Some(res) = existing_map.remove(&scanned_id) {
                // It exists. Did its state change from Removed back to active?
                if res.state == ResourceState::Removed {
                    self.resource_repo
                        .update_state(res.id, ResourceState::PendingUser)?;
                    self.event_repo.insert(&DiscoveryEvent {
                        id: Uuid::new_v4(),
                        vm_id,
                        resource_id: Some(res.id),
                        event_type: DiscoveryEventType::Reappeared,
                        discovered_at: Utc::now(),
                        summary: format!("Resource {} reappeared", scanned_id),
                    })?;
                    info!("Resource {} REAPPEARED", scanned_id);
                } else {
                    // Resource is already Discovered, Monitored, or Ignored.
                    // We preserve its state. No action needed.
                    debug!(
                        "Resource {} remains unchanged in state {:?}",
                        scanned_id, res.state
                    );
                }
            } else {
                // New resource
                let new_id = Uuid::new_v4();
                let new_resource = Resource {
                    id: new_id,
                    vm_id,
                    kind: kind.to_string(),
                    identifier: scanned_id.clone(),
                    state: ResourceState::Discovered,
                    version: 1,
                    created_at: Utc::now(),
                    updated_at: Utc::now(),
                    deleted_at: None,
                };

                self.resource_repo.insert(&new_resource)?;
                self.event_repo.insert(&DiscoveryEvent {
                    id: Uuid::new_v4(),
                    vm_id,
                    resource_id: Some(new_id),
                    event_type: DiscoveryEventType::Discovered,
                    discovered_at: Utc::now(),
                    summary: format!("Discovered new {} resource: {}", kind, scanned_id),
                })?;

                info!("DISCOVERED NEW RESOURCE: [{}] {}", kind, scanned_id);
            }
        }

        // 3. Any items remaining in existing_map were NOT found in the current scan
        for (id_str, res) in existing_map {
            // Only mark as disappeared if it wasn't already removed
            if res.state != ResourceState::Removed {
                self.resource_repo
                    .update_state(res.id, ResourceState::Removed)?;
                self.event_repo.insert(&DiscoveryEvent {
                    id: Uuid::new_v4(),
                    vm_id,
                    resource_id: Some(res.id),
                    event_type: DiscoveryEventType::Disappeared,
                    discovered_at: Utc::now(),
                    summary: format!("Resource {} disappeared from VM", id_str),
                })?;
                info!("Resource {} DISAPPEARED", id_str);
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::Connection;

    // We create a mock setup that initializes an in-memory SQLite DB
    // and returns the repos to test reconciliation.
    fn setup_test_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        crate::db::sqlite::run_migrations(&mut conn).unwrap();
        // Insert a dummy VM
        conn.execute(
            "INSERT INTO vms (id, proxmox_vmid, node_name, name, created_at, updated_at) 
             VALUES ('00000000-0000-0000-0000-000000000000', 101, 'node', 'vm', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')", 
            []
        ).unwrap();
        conn
    }

    // Note: To make DiscoveryEngine testable without a real HTTP client,
    // we would extract ProxmoxClient into a trait.
    // However, since econcile_resources only relies on the repositories,
    // we can test the reconciliation logic directly if we extract it or make it public.
    // For this demonstration, we assume it's accessible.
}
