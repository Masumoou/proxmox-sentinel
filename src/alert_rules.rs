use crate::alerts::Alert;
use crate::config::AlertRuleConfig;
use crate::proxmox_api::{GuestKind, GuestStatus, NodeStatus, StorageStatus};
use std::collections::HashMap;

#[derive(Default)]
pub struct AlertRuleEvaluator {
    first_seen: HashMap<String, i64>,
    #[cfg(test)]
    now_override: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceRuleState {
    pub name: String,
    pub state: String,
    pub sub_state: String,
}

#[derive(Debug, Clone)]
pub struct GuestDiskRuleMount {
    pub mountpoint: String,
    pub used: u64,
    pub total: u64,
    pub use_pct: f64,
}

impl ServiceRuleState {
    pub fn new(
        name: impl Into<String>,
        state: impl Into<String>,
        sub_state: impl Into<String>,
    ) -> Self {
        Self {
            name: normalize_service_name(&name.into()),
            state: normalize_service_state(&state.into()),
            sub_state: normalize_service_state(&sub_state.into()),
        }
    }

    pub fn running(&self) -> bool {
        matches!(self.state.as_str(), "active" | "running" | "started")
            && matches!(self.sub_state.as_str(), "running" | "started" | "active")
    }

    pub fn failed(&self) -> bool {
        self.state == "failed" || self.sub_state == "failed"
    }

    pub fn inactive(&self) -> bool {
        self.state == "inactive" || self.sub_state == "inactive" || self.sub_state == "dead"
    }

    pub fn dead(&self) -> bool {
        self.sub_state == "dead"
    }

    pub fn activating(&self) -> bool {
        self.state == "activating" || self.sub_state == "activating"
    }

    pub fn label(&self) -> String {
        if self.sub_state.is_empty() || self.state == self.sub_state {
            self.state.clone()
        } else {
            format!("{}/{}", self.state, self.sub_state)
        }
    }
}

impl AlertRuleEvaluator {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    fn set_now(&mut self, now: i64) {
        self.now_override = Some(now);
    }

    fn now(&self) -> i64 {
        #[cfg(test)]
        if let Some(now) = self.now_override {
            return now;
        }
        chrono::Utc::now().timestamp()
    }

    pub fn evaluate_node(&mut self, rules: &[AlertRuleConfig], node: &NodeStatus) -> Vec<Alert> {
        let now = self.now();
        let mut alerts = Vec::new();
        for rule in rules
            .iter()
            .filter(|rule| rule.enabled && target_is(rule, "node"))
        {
            if rule
                .node
                .as_deref()
                .is_some_and(|wanted| wanted != node.node)
            {
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
            if let Some(alert) =
                self.duration_alert(rule, &format!("node:{}", node.node), condition, detail, now)
            {
                alerts.push(alert);
            }
        }
        alerts
    }

    pub fn evaluate_guest(&mut self, rules: &[AlertRuleConfig], guest: &GuestStatus) -> Vec<Alert> {
        let now = self.now();
        let mut alerts = Vec::new();
        for rule in rules
            .iter()
            .filter(|rule| rule.enabled && matches_guest_target(rule, guest))
        {
            if rule.vmid.is_some_and(|wanted| wanted != guest.vmid) {
                continue;
            }
            if rule
                .node
                .as_deref()
                .is_some_and(|wanted| wanted != guest.node)
            {
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
            if let Some(alert) = self.duration_alert(
                rule,
                &format!("guest:{}", guest.vmid),
                condition,
                detail,
                now,
            ) {
                alerts.push(alert);
            }
        }
        alerts
    }

    pub fn evaluate_storage(
        &mut self,
        rules: &[AlertRuleConfig],
        storage: &StorageStatus,
    ) -> Vec<Alert> {
        let now = self.now();
        let mut alerts = Vec::new();
        for rule in rules
            .iter()
            .filter(|rule| rule.enabled && target_is(rule, "storage"))
        {
            if rule
                .node
                .as_deref()
                .is_some_and(|wanted| wanted != storage.node)
            {
                continue;
            }
            if rule
                .storage
                .as_deref()
                .is_some_and(|wanted| wanted != storage.storage)
            {
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

    pub fn evaluate_guest_disks(
        &mut self,
        rules: &[AlertRuleConfig],
        vmid: u32,
        node: &str,
        mounts: &[GuestDiskRuleMount],
    ) -> Vec<Alert> {
        let now = self.now();
        let mut alerts = Vec::new();
        for rule in rules
            .iter()
            .filter(|rule| rule.enabled && target_is(rule, "guest_disk"))
        {
            if rule.vmid.is_some_and(|wanted| wanted != vmid) {
                continue;
            }
            if rule.node.as_deref().is_some_and(|wanted| wanted != node) {
                continue;
            }

            let selected = if let Some(wanted_mount) = rule.mount.as_deref() {
                mounts.iter().find(|mount| mount.mountpoint == wanted_mount)
            } else {
                mounts.iter().max_by(|a, b| a.use_pct.total_cmp(&b.use_pct))
            };

            let Some(mount) = selected else {
                if let Some(alert) = self.duration_alert(
                    rule,
                    &format!("guest_disk:{vmid}:missing"),
                    false,
                    "guest disk data is unavailable".to_string(),
                    now,
                ) {
                    alerts.push(alert);
                }
                continue;
            };

            let metric = rule.metric.as_deref().unwrap_or("used_percent");
            let value = match metric {
                "used_percent" | "usage" | "use_pct" | "used_pct" => mount.use_pct,
                "free_percent" | "avail_percent" => 100.0 - mount.use_pct,
                "used_bytes" | "used" => mount.used as f64,
                "total_bytes" | "total" => mount.total as f64,
                _ => continue,
            };
            let (condition, detail) = compare_number(rule, value);
            if let Some(alert) = self.duration_alert(
                rule,
                &format!("guest_disk:{vmid}:{}", mount.mountpoint),
                condition,
                format!("mount {} {} is {:.1}", mount.mountpoint, metric, value),
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
        services: &HashMap<String, ServiceRuleState>,
    ) -> Vec<Alert> {
        let now = self.now();
        let mut alerts = Vec::new();
        for rule in rules
            .iter()
            .filter(|rule| rule.enabled && target_is(rule, "service"))
        {
            if rule.vmid.is_some_and(|wanted| wanted != vmid) {
                continue;
            }
            if rule.node.as_deref().is_some_and(|wanted| wanted != node) {
                continue;
            }
            let Some(service) = rule.service.as_deref().map(normalize_service_name) else {
                continue;
            };
            let state = services.get(&service);
            let condition_name = rule.condition.as_deref().unwrap_or("down");
            let condition = service_condition_matches(condition_name, state);
            let detail = match state {
                Some(state) => format!("service {service} is {}", state.label()),
                None => format!("service {service} is missing"),
            };
            if let Some(alert) = self.duration_alert(
                rule,
                &format!("service:{vmid}:{service}"),
                condition,
                detail,
                now,
            ) {
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
            scope: scope.to_string(),
            severity: rule.severity.as_str().to_string(),
            summary: format!(
                "Custom alert rule '{}' matched {scope}: {detail}",
                rule.name
            ),
        })
    }
}

pub fn normalize_service_name(name: &str) -> String {
    name.strip_suffix(".service")
        .unwrap_or(name)
        .to_ascii_lowercase()
}

pub fn service_state_map<I>(services: I) -> HashMap<String, ServiceRuleState>
where
    I: IntoIterator<Item = ServiceRuleState>,
{
    services
        .into_iter()
        .map(|service| (normalize_service_name(&service.name), service))
        .collect()
}

fn service_condition_matches(condition: &str, state: Option<&ServiceRuleState>) -> bool {
    let condition = condition.to_ascii_lowercase();
    match condition.as_str() {
        "running" | "up" | "active" => state.is_some_and(ServiceRuleState::running),
        "failed" => state.is_some_and(ServiceRuleState::failed),
        "inactive" => state.is_some_and(ServiceRuleState::inactive),
        "dead" => state.is_some_and(ServiceRuleState::dead),
        "activating" => state.is_some_and(ServiceRuleState::activating),
        "missing" => state.is_none(),
        "unknown" => state
            .map(|s| s.state == "unknown" || s.sub_state == "unknown")
            .unwrap_or(true),
        "down" | "not_running" | "stopped" => !state.is_some_and(ServiceRuleState::running),
        _ => false,
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
    (
        matched,
        format!("actual {actual:.1} {op} threshold {threshold:.1}"),
    )
}

fn compare_text(rule: &AlertRuleConfig, actual: &str) -> (bool, String) {
    let expected = rule.value.as_deref().unwrap_or("");
    let op = rule.operator.as_deref().unwrap_or("==");
    let matched = match op {
        "==" | "=" => actual.eq_ignore_ascii_case(expected),
        "!=" => !actual.eq_ignore_ascii_case(expected),
        _ => false,
    };
    (
        matched,
        format!("actual '{actual}' {op} expected '{expected}'"),
    )
}

fn normalize_service_state(state: &str) -> String {
    let state = state.trim().to_ascii_lowercase();
    if state.is_empty() {
        "unknown".to_string()
    } else {
        state
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::AlertSeverity;
    use crate::proxmox_api::{GuestKind, GuestStatus, NodeStatus, StorageStatus};

    fn rule(name: &str, target: &str) -> AlertRuleConfig {
        AlertRuleConfig {
            enabled: true,
            name: name.to_string(),
            target: target.to_string(),
            vmid: None,
            node: None,
            storage: None,
            service: None,
            mount: None,
            metric: None,
            operator: None,
            threshold: None,
            value: None,
            condition: None,
            notification_channel: None,
            notes: None,
            duration_secs: 0,
            severity: AlertSeverity::Warning,
        }
    }

    fn node() -> NodeStatus {
        NodeStatus {
            node: "pve1".into(),
            status: "online".into(),
            cpu_usage: 0.91,
            cpu_count: 8,
            mem_used: 91,
            mem_total: 100,
            swap_used: 0,
            swap_total: 0,
            disk_used: 20,
            disk_total: 100,
            load_avg1: 0.0,
            load_avg5: 0.0,
            load_avg15: 0.0,
            uptime: 0,
            kernel_version: "6.17.2-1-pve".into(),
            pve_version: "pve-manager/9.1.1".into(),
        }
    }

    fn guest(kind: GuestKind, status: &str) -> GuestStatus {
        GuestStatus {
            vmid: 101,
            name: "web".into(),
            kind,
            status: status.into(),
            cpu_usage: 0.90,
            cpu_count: 4,
            mem_used: 70,
            mem_total: 100,
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
        }
    }

    fn storage() -> StorageStatus {
        StorageStatus {
            node: "pve1".into(),
            storage: "local-lvm".into(),
            kind: "lvmthin".into(),
            content: "images,rootdir".into(),
            active: true,
            enabled: true,
            total: 100,
            used: 86,
            avail: 14,
        }
    }

    #[test]
    fn duration_waits_then_fires() {
        let mut evaluator = AlertRuleEvaluator::new();
        evaluator.set_now(100);
        let mut rule = rule("node-cpu", "node");
        rule.metric = Some("cpu".into());
        rule.operator = Some(">".into());
        rule.threshold = Some(80.0);
        rule.duration_secs = 30;

        assert!(evaluator.evaluate_node(&[rule.clone()], &node()).is_empty());
        evaluator.set_now(129);
        assert!(evaluator.evaluate_node(&[rule.clone()], &node()).is_empty());
        evaluator.set_now(130);
        assert_eq!(evaluator.evaluate_node(&[rule], &node()).len(), 1);
    }

    #[test]
    fn duration_resets_when_condition_clears() {
        let mut evaluator = AlertRuleEvaluator::new();
        evaluator.set_now(100);
        let mut rule = rule("node-memory", "node");
        rule.metric = Some("memory".into());
        rule.operator = Some(">".into());
        rule.threshold = Some(90.0);
        rule.duration_secs = 30;
        let mut n = node();

        assert!(evaluator.evaluate_node(&[rule.clone()], &n).is_empty());
        n.mem_used = 10;
        evaluator.set_now(120);
        assert!(evaluator.evaluate_node(&[rule.clone()], &n).is_empty());
        n.mem_used = 91;
        evaluator.set_now(140);
        assert!(evaluator.evaluate_node(&[rule.clone()], &n).is_empty());
        evaluator.set_now(170);
        assert_eq!(evaluator.evaluate_node(&[rule], &n).len(), 1);
    }

    #[test]
    fn evaluates_node_cpu_and_memory_rules() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut cpu = rule("node-cpu", "node");
        cpu.metric = Some("cpu".into());
        cpu.operator = Some(">".into());
        cpu.threshold = Some(80.0);
        let mut memory = rule("node-memory", "node");
        memory.metric = Some("memory".into());
        memory.operator = Some(">".into());
        memory.threshold = Some(90.0);

        assert_eq!(evaluator.evaluate_node(&[cpu, memory], &node()).len(), 2);
    }

    #[test]
    fn evaluates_guest_status_rule() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut stopped = rule("vm-stopped", "vm");
        stopped.metric = Some("status".into());
        stopped.operator = Some("==".into());
        stopped.value = Some("stopped".into());

        assert_eq!(
            evaluator
                .evaluate_guest(&[stopped], &guest(GuestKind::Vm, "stopped"))
                .len(),
            1
        );
    }

    #[test]
    fn evaluates_vm_cpu_rule() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut cpu = rule("vm-cpu", "vm");
        cpu.vmid = Some(101);
        cpu.metric = Some("cpu".into());
        cpu.operator = Some(">".into());
        cpu.threshold = Some(86.0);

        assert_eq!(
            evaluator
                .evaluate_guest(&[cpu], &guest(GuestKind::Vm, "running"))
                .len(),
            1
        );
    }

    #[test]
    fn evaluates_storage_usage_rule() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut usage = rule("storage-usage", "storage");
        usage.metric = Some("usage".into());
        usage.operator = Some(">".into());
        usage.threshold = Some(85.0);

        assert_eq!(evaluator.evaluate_storage(&[usage], &storage()).len(), 1);
    }

    #[test]
    fn evaluates_guest_disk_rule_for_root_mount() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut disk = rule("root-disk-high", "guest_disk");
        disk.vmid = Some(101);
        disk.mount = Some("/".into());
        disk.metric = Some("used_percent".into());
        disk.operator = Some(">".into());
        disk.threshold = Some(85.0);
        let mounts = [GuestDiskRuleMount {
            mountpoint: "/".into(),
            used: 90,
            total: 100,
            use_pct: 90.0,
        }];

        assert_eq!(
            evaluator
                .evaluate_guest_disks(&[disk], 101, "pve1", &mounts)
                .len(),
            1
        );
    }

    #[test]
    fn evaluates_service_down_and_missing_rules() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut down = rule("nginx-down", "service");
        down.vmid = Some(101);
        down.service = Some("nginx.service".into());
        down.condition = Some("down".into());
        let services = service_state_map([ServiceRuleState::new("nginx", "failed", "failed")]);

        assert_eq!(
            evaluator
                .evaluate_services(&[down], 101, "pve1", &services)
                .len(),
            1
        );

        let mut missing = rule("postgres-missing", "service");
        missing.service = Some("postgresql".into());
        missing.condition = Some("missing".into());
        assert_eq!(
            evaluator
                .evaluate_services(&[missing], 101, "pve1", &services)
                .len(),
            1
        );
    }

    #[test]
    fn evaluates_service_up_rule() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut up = rule("nginx-up", "service");
        up.service = Some("nginx".into());
        up.condition = Some("running".into());
        let services =
            service_state_map([ServiceRuleState::new("nginx.service", "active", "running")]);

        assert_eq!(
            evaluator
                .evaluate_services(&[up], 101, "pve1", &services)
                .len(),
            1
        );
    }

    #[test]
    fn evaluates_dead_service_condition() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut dead = rule("nginx-dead", "service");
        dead.service = Some("nginx".into());
        dead.condition = Some("dead".into());
        let services =
            service_state_map([ServiceRuleState::new("nginx.service", "inactive", "dead")]);

        assert_eq!(
            evaluator
                .evaluate_services(&[dead], 101, "pve1", &services)
                .len(),
            1
        );
    }

    #[test]
    fn supports_lte_and_not_equal_operators() {
        let mut evaluator = AlertRuleEvaluator::new();
        let mut lte = rule("storage-low", "storage");
        lte.metric = Some("usage".into());
        lte.operator = Some("<=".into());
        lte.threshold = Some(86.0);
        let mut ne = rule("guest-not-running", "vm");
        ne.metric = Some("status".into());
        ne.operator = Some("!=".into());
        ne.value = Some("running".into());

        assert_eq!(evaluator.evaluate_storage(&[lte], &storage()).len(), 1);
        assert_eq!(
            evaluator
                .evaluate_guest(&[ne], &guest(GuestKind::Vm, "stopped"))
                .len(),
            1
        );
    }
}
