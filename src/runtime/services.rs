use std::collections::HashMap;

use crate::alert_rules::{ServiceRuleState, normalize_service_name};
use crate::alerts::Alert;
use crate::config::{Config, ServicesConfig};

pub(super) fn service_is_healthy(state: &str, sub_state: &str) -> bool {
    matches!(state, "active" | "started") && matches!(sub_state, "running" | "started" | "active")
}

pub(super) fn is_public_bind_without_auth(cfg: &Config) -> bool {
    let auth_empty = cfg
        .metrics
        .auth
        .as_deref()
        .map(str::trim)
        .unwrap_or("")
        .is_empty();
    let public_bind = matches!(
        cfg.metrics.listen_addr.as_str(),
        "0.0.0.0" | "::" | "[::]" | ""
    );
    auth_empty && public_bind
}

pub(super) fn service_auto_watch_enabled(cfg: &ServicesConfig) -> bool {
    cfg.auto_watch_running_services || cfg.alert_on_previously_running_down
}

pub(super) fn critical_pattern_alerts(
    patterns: &[String],
    vmid: u32,
    node: &str,
    services: &HashMap<String, ServiceRuleState>,
) -> Vec<Alert> {
    let mut alerts = Vec::new();
    for pattern in patterns.iter().map(|p| p.trim()).filter(|p| !p.is_empty()) {
        let mut matched = services
            .iter()
            .filter(|(name, _)| service_matches_pattern(name, pattern))
            .peekable();

        if matched.peek().is_none() {
            alerts.push(Alert::ServiceUnavailable {
                vmid,
                node: node.to_string(),
                service: normalize_service_name(pattern),
            });
            continue;
        }

        for (name, state) in matched {
            if !state.running() {
                alerts.push(Alert::ServiceUnavailable {
                    vmid,
                    node: node.to_string(),
                    service: name.clone(),
                });
            }
        }
    }
    alerts
}

pub(super) fn service_matches_pattern(name: &str, pattern: &str) -> bool {
    let name = normalize_service_name(name);
    let pattern = normalize_service_name(pattern);
    if pattern == name {
        return true;
    }
    if !pattern.contains('*') {
        return false;
    }

    let mut remaining = name.as_str();
    let mut parts = pattern.split('*').peekable();
    let anchored_start = !pattern.starts_with('*');
    let anchored_end = !pattern.ends_with('*');

    if let Some(first) = parts.next() {
        if anchored_start && !remaining.starts_with(first) {
            return false;
        }
        if !first.is_empty() {
            remaining = &remaining[first.len().min(remaining.len())..];
        }
    }

    let mut last_part = "";
    for part in parts {
        last_part = part;
        if part.is_empty() {
            continue;
        }
        let Some(idx) = remaining.find(part) else {
            return false;
        };
        remaining = &remaining[idx + part.len()..];
    }

    !anchored_end || remaining.is_empty() || name.ends_with(last_part)
}

#[cfg(test)]
mod tests {
    use super::{critical_pattern_alerts, service_auto_watch_enabled, service_matches_pattern};
    use crate::alert_rules::{ServiceRuleState, service_state_map};
    use crate::config::ServicesConfig;

    #[test]
    fn service_auto_watch_defaults_to_false() {
        assert!(!service_auto_watch_enabled(&ServicesConfig::default()));
        let mut old_config = ServicesConfig {
            alert_on_discovered: true,
            ..ServicesConfig::default()
        };
        assert!(!service_auto_watch_enabled(&old_config));
        old_config.auto_watch_running_services = true;
        assert!(service_auto_watch_enabled(&old_config));
    }

    #[test]
    fn critical_patterns_are_empty_by_default() {
        let services =
            service_state_map([ServiceRuleState::new("fwupd.service", "inactive", "dead")]);
        assert!(critical_pattern_alerts(&[], 104, "pve1", &services).is_empty());
    }

    #[test]
    fn service_pattern_supports_exact_and_wildcards() {
        assert!(service_matches_pattern("php8.3-fpm.service", "php*-fpm"));
        assert!(service_matches_pattern("apache2.service", "apache2"));
        assert!(!service_matches_pattern("fwupd.service", "php*-fpm"));
    }
}
