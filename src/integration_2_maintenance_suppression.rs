use chrono::{Duration, TimeZone, Utc};
use rusqlite::Connection;
use std::str::FromStr;
use uuid::Uuid;

use crate::db::sqlite::repository::*;
use crate::db::sqlite::run_migrations;
use crate::domain::alert::{Alert, AlertState};
use crate::domain::incident::{Incident, IncidentState};
use crate::domain::maintenance::{MaintenanceScopeType, MaintenanceWindow};
use crate::domain::metric::{Metric, MetricValueType};
use crate::domain::monitor::{ConfigState, Monitor};
use crate::domain::notification::NotificationRoute;
use crate::domain::resource::{Resource, ResourceState};
use crate::domain::rule::{Operator, Rule};
use crate::domain::telemetry::{ObservationState, Telemetry};
use crate::intelligence::correlation_engine::CorrelationEngine;
use crate::intelligence::inhibition_engine::InhibitionEngine;
use crate::intelligence::maintenance_engine::MaintenanceEngine;
use crate::intelligence::notification_engine::NotificationEngine;
use crate::intelligence::rule_engine::RuleEngine;

fn setup_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn).unwrap();
    conn
}

#[test]
fn test_integration_2_maintenance_suppression() {
    let conn = setup_db();

    let res_repo = ResourceRepository::new(&conn);
    let mon_repo = MonitorRepository::new(&conn);
    let met_repo = MetricRepository::new(&conn);
    let rule_repo = RuleRepository::new(&conn);
    let tel_repo = TelemetryRepository::new(&conn);
    let alert_repo = AlertRepository::new(&conn);
    let inc_repo = IncidentRepository::new(&conn);
    let notif_repo = NotificationRepository::new(&conn);
    let route_repo = NotificationRouteRepository::new(&conn);
    let corr_repo = IncidentCorrelationRepository::new(&conn);
    let maint_repo = MaintenanceWindowRepository::new(&conn);

    let maint_eng = MaintenanceEngine::new(
        &maint_repo,
        &alert_repo,
        &rule_repo,
        &met_repo,
        &mon_repo,
        &res_repo,
    );
    let corr_eng = CorrelationEngine::new(&inc_repo, &corr_repo);
    let inhib_eng = InhibitionEngine::new(&inc_repo, &corr_repo);

    let rule_eng = RuleEngine::new(
        &rule_repo,
        &tel_repo,
        &alert_repo,
        &inc_repo,
        &met_repo,
        &mon_repo,
    );
    let notif_eng = NotificationEngine::new(&notif_repo, &route_repo, &maint_eng, &inhib_eng);

    // STEP 1: Prepare configuration hierarchy
    let vm_id = Uuid::new_v4(); // VM 101

    let resource_id = Uuid::new_v4();
    res_repo
        .insert(&Resource {
            id: resource_id,
            vm_id,
            kind: "Service".to_string(),
            identifier: "nginx.service".to_string(),
            state: ResourceState::Monitored,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        })
        .unwrap();

    let monitor_id = Uuid::new_v4();
    conn.execute("INSERT INTO monitors (id, resource_id, state, interval_secs, collection_type, version, created_at, updated_at) VALUES (?1, ?2, 'ENABLED', 30, 'Systemd', 1, ?3, ?4)",
        rusqlite::params![monitor_id.to_string(), resource_id.to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()]).unwrap();

    let metric_id = Uuid::new_v4();
    conn.execute("INSERT INTO metrics (id, monitor_id, name, value_type, unit) VALUES (?1, ?2, 'status', 'state', 'state')",
        rusqlite::params![metric_id.to_string(), monitor_id.to_string()]).unwrap();

    let rule_id = Uuid::new_v4();
    conn.execute("INSERT INTO rules (id, metric_id, template_id, state, fire_operator, fire_value_type, fire_value, fire_duration_secs, resolve_operator, resolve_value_type, resolve_value, resolve_duration_secs, severity, version, created_at, updated_at) VALUES (?1, ?2, NULL, 'ENABLED', 'EQUAL', 'string', 'inactive', 120, 'EQUAL', 'string', 'active', 120, 'Critical', 1, ?3, ?4)",
        rusqlite::params![rule_id.to_string(), metric_id.to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()]).unwrap();

    let template_id = Uuid::new_v4();
    conn.execute("INSERT INTO templates (id, name, title, body, format, version, created_at, updated_at) VALUES (?1, 'Critical Service Alert', '🚨 {{severity}}: {{resource_name}}', '{{resource_name}} on {{vm_name}} is {{value}}.', 'markdown', 1, ?2, ?3)",
        rusqlite::params![template_id.to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()]).unwrap();

    let channel_id = Uuid::new_v4();
    conn.execute("INSERT INTO notification_channels (id, name, channel_type, state, configuration, version, created_at, updated_at) VALUES (?1, 'Test Webhook', 'Webhook', 'ENABLED', '{}', 1, ?2, ?3)",
        rusqlite::params![channel_id.to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()]).unwrap();

    let route_id = Uuid::new_v4();
    let route = NotificationRoute {
        id: route_id,
        name: "Route 1".to_string(),
        rule_id: Some(rule_id),
        severity: Some("Critical".to_string()),
        scope_type: Some("VM".to_string()),
        scope_id: Some(vm_id),
        priority: 100,
        template_id,
        channel_id,
        state: ConfigState::Enabled,
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted_at: None,
    };
    route_repo.insert(&route).unwrap();

    // STEP 2: Create a Maintenance Window for the VM
    let window = MaintenanceWindow {
        id: Uuid::new_v4(),
        scope_type: MaintenanceScopeType::Vm,
        scope_id: Some(vm_id),
        start_time: Utc::now() - Duration::hours(1), // Started 1 hr ago
        end_time: Utc::now() + Duration::hours(1),   // Ends in 1 hr
        created_by: "admin".to_string(),
    };
    maint_repo.insert(&window).unwrap();

    // STEP 3: Create an Alert and Incident (simulating what RuleEngine would do)
    let alert = Alert {
        id: Uuid::new_v4(),
        rule_id,
        state: AlertState::Firing,
        fired_at: Some(Utc::now()),
        resolved_at: None,
    };
    alert_repo.insert(&alert).unwrap();

    let incident = Incident {
        id: Uuid::new_v4(),
        alert_id: alert.id,
        vm_id,
        state: IncidentState::Open,
        started_at: Utc::now(),
        acknowledged_at: None,
        resolved_at: None,
        acknowledged_by: None,
        root_cause_summary: None,
    };
    inc_repo.insert(&incident).unwrap();

    // Verify incident is in DB
    let active_inc = inc_repo
        .get_active_by_alert(alert.id)
        .unwrap()
        .expect("Incident should be active");
    assert_eq!(active_inc.state, IncidentState::Open);

    // STEP 4: Process the incident through NotificationEngine
    // This should detect the MaintenanceWindow and suppress the notification
    notif_eng
        .process_incident(
            &active_inc,
            Some(rule_id),
            Some("Critical"),
            Some(vm_id),
            Some(resource_id),
            Some(metric_id),
        )
        .unwrap();

    // STEP 5: Verify no notifications were generated
    let notifs_count: i64 = conn
        .query_row("SELECT COUNT(*) FROM notifications", [], |row| row.get(0))
        .unwrap();
    assert_eq!(
        notifs_count, 0,
        "Notification should be suppressed by MaintenanceWindow"
    );

    // The incident should STILL be open (visible in UI, just no alerts sent)
    let active_inc_after = inc_repo
        .get_active_by_alert(alert.id)
        .unwrap()
        .expect("Incident should still be active");
    assert_eq!(active_inc_after.state, IncidentState::Open);
}
