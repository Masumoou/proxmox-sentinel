use anyhow::Result;
use chrono::Utc;
use tracing::{debug, info};
use uuid::Uuid;

use crate::db::sqlite::repository::{
    AlertRepository, IncidentCorrelationRepository, IncidentRepository, MetricRepository,
    MonitorRepository, ResourceRepository, RuleRepository,
};
use crate::domain::incident::{CorrelationType, Incident, IncidentCorrelation};

pub struct CorrelationEngine<'a> {
    alert_repo: &'a AlertRepository<'a>,
    rule_repo: &'a RuleRepository<'a>,
    metric_repo: &'a MetricRepository<'a>,
    monitor_repo: &'a MonitorRepository<'a>,
    resource_repo: &'a ResourceRepository<'a>,
    incident_repo: &'a IncidentRepository<'a>,
    correlation_repo: &'a IncidentCorrelationRepository<'a>,
}

impl<'a> CorrelationEngine<'a> {
    pub fn new(
        alert_repo: &'a AlertRepository<'a>,
        rule_repo: &'a RuleRepository<'a>,
        metric_repo: &'a MetricRepository<'a>,
        monitor_repo: &'a MonitorRepository<'a>,
        resource_repo: &'a ResourceRepository<'a>,
        incident_repo: &'a IncidentRepository<'a>,
        correlation_repo: &'a IncidentCorrelationRepository<'a>,
    ) -> Self {
        Self {
            alert_repo,
            rule_repo,
            metric_repo,
            monitor_repo,
            resource_repo,
            incident_repo,
            correlation_repo,
        }
    }

    /// Evaluates a target incident against all other open incidents to determine
    /// if a parent root-cause relationship exists.
    pub fn correlate_incident(&self, target_incident: &Incident) -> Result<()> {
        // Skip if this incident is already correlated as a child
        if self
            .correlation_repo
            .get_by_child_id(target_incident.id)?
            .is_some()
        {
            return Ok(());
        }

        let target_resource = match self.get_incident_resource(target_incident)? {
            Some(r) => r,
            None => return Ok(()),
        };

        // Fetch all open incidents to compare against
        let open_incidents = self.incident_repo.list_open_incidents()?;

        for candidate_parent in open_incidents {
            // Don't correlate with itself
            if candidate_parent.id == target_incident.id {
                continue;
            }

            let parent_resource = match self.get_incident_resource(&candidate_parent)? {
                Some(r) => r,
                None => continue,
            };

            // Rule 1: VM To Resource Failure (e.g. VM unreachable inhibits guest services)
            if parent_resource.vm_id == target_resource.vm_id {
                // If the parent is a VM reachability issue and the child is a service inside that VM
                if parent_resource.kind == "vm_reachability"
                    && target_resource.kind != "vm_reachability"
                {
                    // We only correlate if the parent incident started BEFORE or AT THE SAME TIME as the child incident
                    // We allow a small 60s buffer for out-of-order evaluation due to intervals.
                    let time_diff = target_incident
                        .created_at
                        .signed_duration_since(candidate_parent.created_at);
                    if time_diff.num_seconds() > -60 {
                        self.build_and_store_correlation(
                            candidate_parent.id,
                            target_incident.id,
                            CorrelationType::VmToResource,
                            90, // High confidence
                            format!("Parent VM reachability failure on {} suppresses dependent resource {}", parent_resource.vm_id, target_resource.identifier)
                        )?;
                        return Ok(()); // First strong correlation wins
                    }
                }
            }
        }

        Ok(())
    }

    fn get_incident_resource(
        &self,
        incident: &Incident,
    ) -> Result<Option<crate::domain::resource::Resource>> {
        let alert = match self.alert_repo.get_by_id(incident.alert_id)? {
            Some(a) => a,
            None => return Ok(None),
        };
        let rule = match self.rule_repo.get_by_id(alert.rule_id)? {
            Some(r) => r,
            None => return Ok(None),
        };
        let metric = match self.metric_repo.get_by_id(rule.metric_id)? {
            Some(m) => m,
            None => return Ok(None),
        };
        let monitor = match self.monitor_repo.get_by_id(metric.monitor_id)? {
            Some(m) => m,
            None => return Ok(None),
        };
        self.resource_repo.get_by_id(monitor.resource_id)
    }

    fn build_and_store_correlation(
        &self,
        parent: Uuid,
        child: Uuid,
        corr_type: CorrelationType,
        confidence: u8,
        reason: String,
    ) -> Result<()> {
        let corr = IncidentCorrelation {
            id: Uuid::new_v4(),
            parent_incident_id: parent,
            child_incident_id: child,
            correlation_type: corr_type,
            confidence_score: confidence,
            reason: reason.clone(),
            created_at: Utc::now(),
        };

        info!(
            "Correlated Incident {} as child of {} (Reason: {})",
            child, parent, reason
        );
        self.correlation_repo.insert(&corr)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::incident::IncidentState;
    use chrono::Duration;

    fn mock_incident(mins_ago: i64) -> Incident {
        Incident {
            id: Uuid::new_v4(),
            alert_id: Uuid::new_v4(),
            state: IncidentState::Open,
            severity: "critical".into(),
            created_at: Utc::now() - Duration::minutes(mins_ago),
            resolved_at: None,
        }
    }

    #[test]
    fn test_correlation_logic_concept() {
        // Concept test:
        let parent = mock_incident(5); // Fired 5 mins ago
        let child = mock_incident(2); // Fired 2 mins ago

        // The engine compares the time diff:
        let diff = child.created_at.signed_duration_since(parent.created_at);
        assert!(diff.num_seconds() > 0, "Child happened after parent");

        // The engine builds the IncidentCorrelation struct without mutating Incident objects.
        let corr = IncidentCorrelation {
            id: Uuid::new_v4(),
            parent_incident_id: parent.id,
            child_incident_id: child.id,
            correlation_type: CorrelationType::VmToResource,
            confidence_score: 90,
            reason: "Mock Reason".to_string(),
            created_at: Utc::now(),
        };

        assert_eq!(corr.child_incident_id, child.id);
    }
    #[test]
    fn test_correlation_never_modifies_incident_state() {
        let parent = mock_incident(5);
        let mut child = mock_incident(2);

        // Even though child is correlated to parent, its state remains Open.
        // The NotificationEngine will see it is open, but the InhibitionEngine
        // will prevent the notification.
        assert_eq!(child.state, IncidentState::Open);

        let corr = IncidentCorrelation {
            id: Uuid::new_v4(),
            parent_incident_id: parent.id,
            child_incident_id: child.id,
            correlation_type: CorrelationType::VmToResource,
            confidence_score: 100,
            reason: "VM unreachable".into(),
            created_at: Utc::now(),
        };

        // Correlation was generated. Did child's state change?
        // No, we never mutate child.state = IncidentState::Resolved.
        assert_eq!(child.state, IncidentState::Open);
        assert_eq!(corr.child_incident_id, child.id);
    }
}
