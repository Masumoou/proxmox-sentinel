// src/intelligence/mod.rs
//
// Analyzes metrics across the cluster (NodePressureAnalyzer)
// making complex decisions like migration suggestions based
// on historical or current data.

use crate::alerts::{Alert, AlertDispatcher};
use crate::config::IntelligenceConfig;
use std::time::Duration;
use tokio::time::interval;
use tracing::{info, warn};

pub async fn run_analyzer(
    cfg: IntelligenceConfig,
    client: std::sync::Arc<crate::proxmox_api::ProxmoxClient>,
    mut dispatcher: AlertDispatcher,
) {
    if !cfg.enabled {
        return;
    }

    info!("Starting Node Pressure Analyzer");
    let mut ticker = interval(Duration::from_secs(60)); // Check every 60s

    loop {
        ticker.tick().await;

        let nodes = match client.list_nodes().await {
            Ok(n) => n,
            Err(e) => {
                warn!("Analyer failed to list nodes: {}", e);
                continue;
            }
        };

        // Gather metrics about all nodes
        let mut node_stats = vec![];
        for node in &nodes {
            if let Ok(status) = client.node_status(node).await {
                // Determine memory percentage
                let mem_used = status.mem_used as f64;
                let mem_total = status.mem_total as f64;
                let mem_pct = mem_used / mem_total * 100.0;
                let free_pct = 100.0 - mem_pct;
                let free_bytes = status.mem_total - status.mem_used;

                node_stats.push((node.clone(), mem_pct, free_pct, free_bytes));
            }
        }

        // Evaluate nodes that are stressed
        for (node, mem_pct, _free_pct, _free_bytes) in &node_stats {
            if *mem_pct >= cfg.critical_mem_pct {
                // Node is under severe pressure
                // Suggest migration to a node with > target_free_mem_pct

                // Find candidates
                let mut suggest_vmid = None;
                let mut target_node = None;
                for (target, _p, free_p, free_bytes_target) in &node_stats {
                    if target != node && *free_p >= cfg.target_free_mem_pct {
                        // Potential candidate
                        // We should also look at VMs. Let's find the largest VM we can move.
                        if let Ok(guests) = client.list_guests(node).await {
                            // Find largest VM that fits in the target's free mem
                            // For simplicity, just grab the first running VM
                            if let Some(guest) = guests.iter().find(|g| {
                                g.status == "running"
                                    && g.kind == crate::proxmox_api::GuestKind::Vm
                                    && g.mem_total < *free_bytes_target as u64
                            }) {
                                suggest_vmid = Some(guest.vmid);
                                target_node = Some(target.clone());
                                break;
                            }
                        }
                    }
                }

                dispatcher
                    .dispatch(Alert::NodePressureCritical {
                        node: node.clone(),
                        mem_pct: *mem_pct,
                        suggest_vmid,
                        target_node,
                    })
                    .await;
            }
        }
    }
}
