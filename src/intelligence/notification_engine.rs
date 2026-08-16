use anyhow::Result;
use uuid::Uuid;
use chrono::Utc;
use tracing::{info, debug, warn};

use crate::domain::incident::{Incident, IncidentState};
use crate::domain::notification::{Notification, NotificationRoute};
use crate::db::sqlite::repository::{NotificationRepository, NotificationRouteRepository};
use crate::intelligence::maintenance_engine::MaintenanceEngine;
use crate::intelligence::inhibition_engine::InhibitionEngine;

pub struct NotificationEngine<'a> {
    notif_repo: &'a NotificationRepository<'a>,
    route_repo: &'a NotificationRouteRepository<'a>,
    maintenance_engine: &'a MaintenanceEngine<'a>,
    inhibition_engine: &'a InhibitionEngine<'a>,
}

impl<'a> NotificationEngine<'a> {
    pub fn new(
        notif_repo: &'a NotificationRepository<'a>,
        route_repo: &'a NotificationRouteRepository<'a>,
        maintenance_engine: &'a MaintenanceEngine<'a>,
        inhibition_engine: &'a InhibitionEngine<'a>,
    ) -> Self {
        Self {
            notif_repo,
            route_repo,
            maintenance_engine,
            inhibition_engine,
        }
    }

    /// Evaluates whether an incident should trigger a notification.
    pub fn process_incident(&self, incident: &Incident, rule_id: Option<Uuid>, vm_id: Option<Uuid>, resource_id: Option<Uuid>, metric_id: Option<Uuid>) -> Result<()> {
        debug!("NotificationEngine evaluating incident: {}", incident.id);
        
        // 1. Maintenance Check (User intentionally silenced notifications)
        if self.maintenance_engine.is_incident_under_maintenance(incident)? {
            debug!("Notification skipped: Incident {} is under maintenance", incident.id);
            return Ok(());
        }

        // 2. Inhibition Check (System identified a root cause that renders this noisy)
        if self.inhibition_engine.is_incident_inhibited(incident)? {
            debug!("Notification skipped: Incident {} is inhibited by a root cause incident", incident.id);
            return Ok(());
        }

        // 3. Routing Policy (Determine channels based on rule tags/severity)
        let active_routes = self.route_repo.list_active()?;
        let mut matched = false;

        for route in active_routes {
            if self.route_matches(&route, incident, rule_id, vm_id, resource_id, metric_id) {
                matched = true;
                self.dispatch_to_channel(incident, &route)?;
            }
        }

        if !matched {
            debug!("No notification routes matched for incident {}", incident.id);
        }

        Ok(())
    }

    fn route_matches(&self, route: &NotificationRoute, incident: &Incident, rule_id: Option<Uuid>, vm_id: Option<Uuid>, resource_id: Option<Uuid>, metric_id: Option<Uuid>) -> bool {
        // A route matches when ALL populated conditions match.
        if let Some(r_id) = route.rule_id {
            if Some(r_id) != rule_id { return false; }
        }
        if let Some(ref sev) = route.severity {
            if sev != &incident.severity { return false; }
        }
        if let Some(ref s_type) = route.scope_type {
            if let Some(s_id) = route.scope_id {
                let current_scope_id = match s_type.as_str() {
                    "GLOBAL" => Some(Uuid::nil()), // Wait, global has no id typically
                    "VM" => vm_id,
                    "RESOURCE" => resource_id,
                    "METRIC" => metric_id,
                    _ => None,
                };
                if s_type != "GLOBAL" && current_scope_id != Some(s_id) {
                    return false;
                }
            }
        }
        true
    }

    fn dispatch_to_channel(&self, incident: &Incident, route: &NotificationRoute) -> Result<()> {
        // 5. Dispatch Notification (Mocking actual network call)
        info!("DISPATCHED NOTIFICATION for Incident {} to Channel {} via Route {}", incident.id, route.channel_id, route.id);

        // 6. Record Notification History
        let record = Notification {
            id: Uuid::new_v4(),
            incident_id: incident.id,
            route_id: route.id,
            channel_id: route.channel_id,
            sent_at: Utc::now(),
            success: true,
            error_message: None,
        };

        self.notif_repo.insert(&record)?;
        Ok(())
    }
}

