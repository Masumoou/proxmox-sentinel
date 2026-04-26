use crate::alerts::Alert;
use crate::config::AlertRuleConfig;
use crate::proxmox_api::{GuestKind, GuestStatus, NodeStatus, StorageStatus};
use std::collections::{HashMap, HashSet};

#[derive(Default)]
pub struct AlertRuleEvaluator {
    first_seen: HashMap<String, i64>,
}

impl AlertRuleEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn evaluate_node(&mut self, rules: &[AlertRuleConfig], node: &NodeStatus) -> Vec<Alert> {
        let now = chrono::Utc::now().timestamp();
        let mut alerts = Vec::new();
        for rule in rules.iter().filter(|rule| rule.enabled && target_is(rule, "node")) {
            if rule.node.as_deref().is_some_and(|wanted| wanted != node.node) {
                continue;
            }
            let Some(metric) = rule.metric.as_deref() else {
                continue;
            };
            let (condition, detail) = match metric {
                "cpu" => compare_number(rule, node.cpu_usage * 100.0),
                "memory" | "mem" => compare_number(rule, pct(node.mem_used, node.mem_total)),
                "disk" | "storage" => compare_number(rule, pct(node.disk_used, node.disk_total)),
                "status" => compare_text(rule, &node.status),
                _ => (false, format!("unsupported node metric '{metric}'")),
            };
            if let Some(alert) = self.duration_alert(rule, &format!("node:{}", node.node), condition, detail, now) {
                alerts.push(alert);
            }
        }
        alerts
    }

    pub fn evaluate_guest(&mut self, rules: &[AlertRuleConfig], guest: &GuestStatus) -> Vec<Alert> {
        let now = chrono::Utc::now().timestamp();
        let mut alerts = Vec::new();
        for rule in rules.iter().filter(|rule| rule.enabled && matches_guest_target(rule, guest)) {
            if rule.vmid.is_some_and(|wanted| wanted != guest.vmid) {
                continue;
            }
            if rule.node.as_deref().is_some_and(|wanted| wanted != guest.node) {
                continue;
            }
            let Some(metric) = rule.metric.as_deref() else {
                continue;
            };
            let (condition, detail) = match metric {
                "cpu" => compare_number(rule, guest.cpu_usage * 100.0),
                "memory" | "mem" => compare_number(rule, pct(guest.mem_used, guest.mem_total)),
                "status" => compare_text(rule, &guest.status),
                _ => (false, format!("unsupported guest metric '{metric}'")),
            };
            if let Some(alert) = self.duration_alert(rule, &format!("guest:{}", guest.vmid), condition, detail, now) {
                alerts.push(alert);
            }
        }
        alerts
    }

    pub fn evaluate_storage(&mut self, rules: &[AlertRuleConfig], storage: &StorageStatus) -> Vec<Alert> {
        let now = chrono::Utc::now().timestamp();
        let mut alerts = Vec::new();
        for rule in rules.iter().filter(|rule| rule.enabled && target_is(rule, "storage")) {
            if rule.node.as_deref().is_some_and(|wanted| wanted != storage.node) {
                continue;
            }
            if rule.storage.as_deref().is_some_and(|wanted| wanted != storage.storage) {
                continue;
            }
            let metric = rule.metric.as_deref().unwrap_or("usage");
            let (condition, detail) = match metric {
                "usage" | "used" | "disk" => compare_number(rule, pct(storage.used, storage.total)),
                "active" => compare_text(rule, if storage.active { "true" } else { "false" }),
                "enabled" => compare_text(rule, if storage.enabled { "true" } else { "false" }),
                _ => (false, format!("unsupported storage metric '{metric}'")),
            };
            if let Some(alert) = self.duration_alert(
                rule,
                &format!("storage:{}:{}", storage.node, storage.storage),
                condition,
                detail,
                now,
            ) {
                alerts.push(alert);
            }
        }
        alerts
    }

    pub fn evaluate_services(
        &mut self,
        rules: &[AlertRuleConfig],
        vmid: u32,
        node: &str,
        active_services: &HashSet<String>,
    ) -> Vec<Alert> {
        let now = chrono::Utc::now().timestamp();
        let mut alerts = Vec::new();
        for rule in rules.iter().filter(|rule| rule.enabled && target_is(rule, "service")) {
            if rule.vmid.is_some_and(|wanted| wanted != vmid) {
                continue;
            }
            if rule.node.as_deref().is_some_and(|wanted| wanted != node) {
                continue;
            }
            let Some(service) = rule.service.as_deref().map(normalize_service_name) else {
                continue;
            };
            let active = active_services.contains(&service);
            let condition_name = rule.condition.as_deref().unwrap_or("down");
            let condition = match condition_name {
                "down" | "stopped" | "failed" => !active,
                "up" | "running" | "active" => active,
                _ => false,
            };
            let detail = format!("service {service} is {}", if active { "running" } else { "down" });
            if let Some(alert) = self.duration_alert(rule, &format!("service:{vmid}:{service}"), condition, detail, now) {
                alerts.push(alert);
            }
        }
        alerts
    }

    fn duration_alert(
        &mut self,
        rule: &AlertRuleConfig,
        scope: &str,
        condition: bool,
        detail: String,
        now: i64,
    ) -> Option<Alert> {
        let key = format!("{}:{scope}", rule.name);
        if !condition {
            self.first_seen.remove(&key);
            return None;
        }
        let first_seen = *self.first_seen.entry(key).or_insert(now);
        if now.saturating_sub(first_seen) < rule.duration_secs as i64 {
            return None;
        }
        Some(Alert::CustomRuleTriggered {
            name: rule.name.clone(),
            severity: rule.severity.as_str().to_string(),
            summary: format!("Custom alert rule '{}' matched {scope}: {detail}", rule.name),
        })
    }
}

fn target_is(rule: &AlertRuleConfig, target: &str) -> bool {
    rule.target.eq_ignore_ascii_case(target)
}

fn matches_guest_target(rule: &AlertRuleConfig, guest: &GuestStatus) -> bool {
    match rule.target.to_lowercase().as_str() {
        "guest" => true,
        "vm" => matches!(guest.kind, GuestKind::Vm),
        "lxc" | "container" => matches!(guest.kind, GuestKind::Lxc),
        _ => false,
    }
}

fn pct(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (used as f64 / total as f64) * 100.0
    }
}

fn compare_number(rule: &AlertRuleConfig, actual: f64) -> (bool, String) {
    let threshold = rule.threshold.unwrap_or(0.0);
    let op = rule.operator.as_deref().unwrap_or(">");
    let matched = match op {
        ">" => actual > threshold,
        ">=" => actual >= threshold,
        "<" => actual < threshold,
        "<=" => actual <= threshold,
        "==" | "=" => (actual - threshold).abs() < f64::EPSILON,
        "!=" => (actual - threshold).abs() >= f64::EPSILON,
        _ => false,
    };
    (matched, format!("actual {actual:.1} {op} threshold {threshold:.1}"))
}

fn compare_text(rule: &AlertRuleConfig, actual: &str) -> (bool, String) {
    let expected = rule.value.as_deref().unwrap_or("");
    let op = rule.operator.as_deref().unwrap_or("==");
    let matched = match op {
        "==" | "=" => actual.eq_ignore_ascii_case(expected),
        "!=" => !actual.eq_ignore_ascii_case(expected),
        _ => false,
    };
    (matched, format!("actual '{actual}' {op} expected '{expected}'"))
}

fn normalize_service_name(name: &str) -> String {
    name.strip_suffix(".service").unwrap_or(name).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AlertSeverity;
    use crate::proxmox_api::GuestKind;

    fn rule(name: &str, target: &str) -> AlertRuleConfig {
        AlertRuleConfig {
            enabled: true,
            name: name.to_string(),
            target: target.to_string(),
            vmid: None,
            node: None,
            storage: None,
            service: None,
            metric: None,
            operator: None,
            threshold: None,
            value: None,
            condition: None,
            duration_secs: 0,
            severity: AlertSeverity::Warning,
        }
    }

    #[test]
    fn evaluates_vm_cpu_rule() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut rule = rule("vm-cpu", "vm");
        rule.vmid = Some(101);
        rule.metric = Some("cpu".into());
        rule.operator = Some(">".into());
        rule.threshold = Some(80.0);
        let guest = GuestStatus {
            vmid: 101,
            name: "web".into(),
            kind: GuestKind::Vm,
            status: "running".into(),
            cpu_usage: 0.90,
            cpu_count: 4,
            mem_used: 0,
            mem_total: 0,
            disk_read: 0,
            disk_write: 0,
            net_in: 0,
            net_out: 0,
            uptime: 0,
            node: "pve1".into(),
            ip_address: None,
            tags: vec![],
            os_name: None,
            os_version: None,
            template: false,
        };
        assert_eq!(evaluator.evaluate_guest(&[rule], &guest).len(), 1);
    }

    #[test]
    fn evaluates_missing_service_as_down() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut rule = rule("nginx-down", "service");
        rule.vmid = Some(101);
        rule.service = Some("nginx.service".into());
        rule.condition = Some("down".into());
        let active_services = HashSet::new();
        assert_eq!(evaluator.evaluate_services(&[rule], 101, "pve1", &active_services).len(), 1);
    }
}
