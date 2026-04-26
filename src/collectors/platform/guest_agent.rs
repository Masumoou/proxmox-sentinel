use super::*;

pub(super) async fn collect_guest_agent_health(
    cfg: &PlatformConfig,
    client: Arc<ProxmoxClient>,
    guests: &[crate::proxmox_api::GuestStatus],
    alerts: &mut Vec<Alert>,
) -> Vec<GuestAgentHealth> {
    let mut rows = Vec::new();
    for guest in guests
        .iter()
        .filter(|g| matches!(g.kind, GuestKind::Vm) && g.status == "running")
        .filter(|g| !cfg.exclude_guest_agent_vmids.contains(&g.vmid))
        .filter(|g| !(cfg.ignore_templates && g.template))
    {
        let (status, detail) = match client.vm_agent_ping(&guest.node, guest.vmid).await {
            Ok(()) => ("ok".to_string(), "guest agent ping OK".to_string()),
            Err(e) => ("warning".to_string(), e.to_string()),
        };

        if status != "ok" {
            alerts.push(platform_alert(
                format!("guest_agent:{}:{}", guest.node, guest.vmid),
                "warning",
                format!("QEMU guest agent not responding for {} ({}) on {}: {}", guest.name, guest.vmid, guest.node, detail),
            ));
        }

        rows.push(GuestAgentHealth {
            vmid: guest.vmid,
            name: guest.name.clone(),
            node: guest.node.clone(),
            status,
            detail,
        });
    }
    rows
}
