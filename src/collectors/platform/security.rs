use super::*;

pub(super) async fn collect_security(
    guest_agents: &[GuestAgentHealth],
    alerts: &mut Vec<Alert>,
) -> Vec<SecurityCheck> {
    let mut checks = Vec::new();

    let sshd = tokio::fs::read_to_string("/etc/ssh/sshd_config").await.unwrap_or_default();
    let root_login = sshd.lines().rev().find_map(|line| {
        let line = line.trim();
        if line.starts_with('#') { return None; }
        line.strip_prefix("PermitRootLogin").map(|v| v.trim().to_string())
    }).unwrap_or_else(|| "default".to_string());
    checks.push(security_check(
        "root_login",
        "Root SSH login",
        if matches!(root_login.as_str(), "yes" | "prohibit-password" | "default") { "warning" } else { "ok" },
        &root_login,
        "Posture finding: review PermitRootLogin and ensure password authentication is disabled if root SSH is allowed",
    ));

    let pveversion = run_cmd("pveversion", &[]).await.unwrap_or_default();
    checks.push(security_check("pve_version", "PVE version", "info", pveversion.trim(), "Informational posture check: installed Proxmox version"));

    let repo_detail = read_repo_files().await;
    let repo_severity = repo_posture_severity(&repo_detail);
    checks.push(security_check("repos", "Repository posture", repo_severity, &repo_detail, "Posture finding: repository choice may be intentional; review support/update policy"));

    let fw = run_cmd("pve-firewall", &["status"]).await.unwrap_or_default();
    checks.push(security_check(
        "firewall",
        "PVE firewall",
        if fw.to_lowercase().contains("disabled") { "warning" } else { "ok" },
        fw.trim(),
        "Posture finding: disabled firewall may be intentional on trusted networks; review exposure model",
    ));

    let no_agent = guest_agents
        .iter()
        .filter(|g| g.status != "ok")
        .count();
    checks.push(security_check(
        "guest_agent_visibility",
        "Guest visibility",
        if no_agent > 0 { "info" } else { "ok" },
        &format!("{no_agent} running QEMU guests have guest agent missing or not responding"),
        "Visibility posture",
    ));

    for check in &checks {
        if matches!(check.severity.as_str(), "warning" | "critical") {
            alerts.push(platform_alert(
                format!("security:{}", check.key),
                &check.severity,
                format!("Posture finding {}: {}", check.label, check.status),
            ));
        }
    }
    checks
}


fn security_check(key: &str, label: &str, severity: &str, status: &str, detail: &str) -> SecurityCheck {
    SecurityCheck {
        key: key.into(),
        label: label.into(),
        severity: severity.into(),
        status: status.into(),
        detail: detail.into(),
    }
}

async fn read_repo_files() -> String {
    let mut text = String::new();
    for path in ["/etc/apt/sources.list", "/etc/apt/sources.list.d/pve-enterprise.sources", "/etc/apt/sources.list.d/pve-no-subscription.sources"] {
        if let Ok(content) = tokio::fs::read_to_string(path).await {
            text.push_str(path);
            text.push('\n');
            text.push_str(&content);
            text.push('\n');
        }
    }
    if text.is_empty() { "no apt repo files readable".into() } else { text.lines().take(20).collect::<Vec<_>>().join(" | ") }
}

fn repo_posture_severity(repo_detail: &str) -> &'static str {
    if repo_detail.contains("pve-enterprise") && !repo_detail.contains("download.proxmox.com/debian/pve") {
        "warning"
    } else {
        "ok"
    }
}

#[cfg(test)]
mod tests {
    use super::repo_posture_severity;

    #[test]
    fn repo_posture_is_warning_for_enterprise_only() {
        let repos = "/etc/apt/sources.list.d/pve-enterprise.sources | URIs: https://enterprise.proxmox.com/debian/pve";
        assert_eq!(repo_posture_severity(repos), "warning");
    }

    #[test]
    fn repo_posture_is_ok_when_no_subscription_repo_present() {
        let repos = "deb http://download.proxmox.com/debian/pve trixie pve-no-subscription";
        assert_eq!(repo_posture_severity(repos), "ok");
    }
}
