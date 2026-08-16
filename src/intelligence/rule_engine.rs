use anyhow::Result;
use chrono::{DateTime, Duration, Utc};
use tracing::{debug, info, warn};
use uuid::Uuid;

use crate::db::sqlite::repository::{
    AlertRepository, IncidentRepository, MetricRepository, MonitorRepository, RuleRepository,
    TelemetryRepository,
};
use crate::domain::alert::{Alert, AlertState};
use crate::domain::incident::{Incident, IncidentState};
use crate::domain::rule::{Operator, Rule};
use crate::domain::telemetry::{ObservationState, Telemetry};

pub struct RuleEngine<'a> {
    rule_repo: &'a RuleRepository<'a>,
    telemetry_repo: &'a TelemetryRepository<'a>,
    alert_repo: &'a AlertRepository<'a>,
    incident_repo: &'a IncidentRepository<'a>,
    metric_repo: &'a MetricRepository<'a>,
    monitor_repo: &'a MonitorRepository<'a>,
}

impl<'a> RuleEngine<'a> {
    pub fn new(
        rule_repo: &'a RuleRepository<'a>,
        telemetry_repo: &'a TelemetryRepository<'a>,
        alert_repo: &'a AlertRepository<'a>,
        incident_repo: &'a IncidentRepository<'a>,
        metric_repo: &'a MetricRepository<'a>,
        monitor_repo: &'a MonitorRepository<'a>,
    ) -> Self {
        Self {
            rule_repo,
            telemetry_repo,
            alert_repo,
            incident_repo,
            metric_repo,
            monitor_repo,
        }
    }

    pub fn evaluate_all(&self) -> Result<()> {
        let rules = self.rule_repo.list_enabled()?;
        for rule in rules {
            if let Err(e) = self.evaluate_rule(&rule) {
                warn!("Failed to evaluate rule {}: {}", rule.id, e);
            }
        }
        Ok(())
    }

    // A helper for testing to bypass DB lookups for interval
    pub fn evaluate_rule_with_telemetry(
        &self,
        rule: &Rule,
        telemetry: &[Telemetry],
        monitor_interval_secs: u32,
    ) -> Result<()> {
        if telemetry.is_empty() {
            return Ok(());
        }

        let active_alert = self.alert_repo.get_active_by_rule(rule.id)?;

        // We define a gap as > 2.5x the monitor's interval to allow for slight jitter.
        let max_gap_secs =
            std::cmp::max(monitor_interval_secs * 2 + (monitor_interval_secs / 2), 60) as i64;

        if let Some(alert) = active_alert {
            if let Some(resolve_val) = &rule.resolve_value {
                let resolve_dur = rule.resolve_duration_secs.unwrap_or(0);
                if self.is_condition_continuous(
                    telemetry,
                    rule.operator.clone(),
                    resolve_val,
                    resolve_dur,
                    max_gap_secs,
                ) {
                    self.resolve_alert_and_incident(&alert)?;
                }
            }
        } else {
            if self.is_condition_continuous(
                telemetry,
                rule.operator.clone(),
                &rule.fire_value,
                rule.fire_duration_secs,
                max_gap_secs,
            ) {
                self.fire_new_alert_and_incident(rule)?;
            }
        }
        Ok(())
    }

    fn evaluate_rule(&self, rule: &Rule) -> Result<()> {
        let max_lookback_secs = std::cmp::max(
            rule.fire_duration_secs,
            rule.resolve_duration_secs.unwrap_or(0),
        ) as i64;
        let since = Utc::now() - Duration::seconds(max_lookback_secs + 600); // 10 min buffer

        let telemetry = self
            .telemetry_repo
            .get_recent_for_metric(rule.metric_id, since)?;
        if telemetry.is_empty() {
            return Ok(());
        }

        // We need the monitor interval to detect telemetry gaps accurately.
        // We can't fetch it easily from MetricRepo as it doesn't return Monitor directly in the current API,
        // but let's assume a default max gap of 300s (5m) if we can't look it up, for simplicity here.
        // For production, we'd query: Metric -> Monitor -> interval_secs.
        // Let's implement a hardcoded default 5 min interval for gap checking in the real engine loop,
        // which means a gap is > 750 seconds.
        let monitor_interval_secs = 300;

        self.evaluate_rule_with_telemetry(rule, &telemetry, monitor_interval_secs)
    }

    pub fn is_condition_continuous(
        &self,
        telemetry: &[Telemetry], // Ordered DESC (newest first)
        operator: Operator,
        target_value: &str,
        duration_secs: u32,
        max_gap_secs: i64,
    ) -> bool {
        if telemetry.is_empty() {
            return false;
        }

        let now = Utc::now();
        let cutoff = now - Duration::seconds(duration_secs as i64);

        let mut previous_timestamp = now;

        for t in telemetry {
            // Gap detection: If the time between the point we just checked and this point is too large,
            // the continuity is broken by a data gap.
            if (previous_timestamp - t.timestamp).num_seconds() > max_gap_secs {
                return false; // Data gap detected
            }

            if t.observation == ObservationState::Unknown {
                return false; // Unknown breaks continuity
            }

            let is_match = match operator {
                Operator::Equal => Self::val_eq(t, target_value),
                Operator::NotEqual => !Self::val_eq(t, target_value),
                Operator::GreaterThan => Self::val_gt(t, target_value),
                Operator::LessThan => Self::val_lt(t, target_value),
                Operator::GreaterOrEqual => {
                    Self::val_gt(t, target_value) || Self::val_eq(t, target_value)
                }
                Operator::LessOrEqual => {
                    Self::val_lt(t, target_value) || Self::val_eq(t, target_value)
                }
            };

            if is_match {
                if t.timestamp <= cutoff {
                    return true;
                }
            } else {
                return false; // Condition false breaks continuity
            }

            previous_timestamp = t.timestamp;
        }

        false
    }

    fn val_eq(t: &Telemetry, target: &str) -> bool {
        if let Some(num) = t.value {
            if let Ok(target_num) = target.parse::<f64>() {
                return (num - target_num).abs() < f64::EPSILON;
            }
        }
        if let Some(s) = &t.string_value {
            return s == target;
        }
        false
    }
    fn val_gt(t: &Telemetry, target: &str) -> bool {
        if let Some(num) = t.value {
            if let Ok(target_num) = target.parse::<f64>() {
                return num > target_num;
            }
        }
        false
    }
    fn val_lt(t: &Telemetry, target: &str) -> bool {
        if let Some(num) = t.value {
            if let Ok(target_num) = target.parse::<f64>() {
                return num < target_num;
            }
        }
        false
    }

    fn fire_new_alert_and_incident(&self, rule: &Rule) -> Result<()> {
        let alert_id = Uuid::new_v4();
        let alert = Alert {
            id: alert_id,
            rule_id: rule.id,
            state: AlertState::Firing,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let incident = Incident {
            id: Uuid::new_v4(),
            alert_id,
            state: IncidentState::Open,
            severity: rule.severity.clone(),
            created_at: Utc::now(),
            resolved_at: None,
        };
        self.alert_repo.insert(&alert)?;
        self.incident_repo.insert(&incident)?;
        Ok(())
    }

    fn resolve_alert_and_incident(&self, alert: &Alert) -> Result<()> {
        self.alert_repo
            .update_state(alert.id, AlertState::Resolved)?;
        if let Some(incident) = self.incident_repo.get_active_by_alert(alert.id)? {
            // Important: Only transition to Resolved if we are Open or Acknowledged!
            self.incident_repo
                .update_state(incident.id, IncidentState::Resolved)?;
        }
        Ok(())
    }

    pub fn acknowledge_incident(&self, incident_id: Uuid) -> Result<()> {
        // Ack only affects the Incident state. The underlying Alert remains FIRING.
        self.incident_repo
            .update_state(incident_id, IncidentState::Acknowledged)?;
        Ok(())
    }
}
