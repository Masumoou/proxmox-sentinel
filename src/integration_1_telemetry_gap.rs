use std::str::FromStr;
use rusqlite::Connection;
use uuid::Uuid;
use chrono::{Utc, Duration, TimeZone};

use crate::db::sqlite::run_migrations;
use crate::db::sqlite::repository::*;
use crate::domain::resource::{Resource, ResourceState};
use crate::domain::monitor::{Monitor, ConfigState};
use crate::domain::metric::{Metric, MetricValueType};
use crate::domain::rule::{Rule, Operator};
use crate::domain::telemetry::{Telemetry, ObservationState};
use crate::domain::notification::NotificationRoute;
use crate::domain::incident::{Incident, IncidentState};
use crate::intelligence::rule_engine::RuleEngine;
use crate::intelligence::notification_engine::NotificationEngine;
use crate::intelligence::maintenance_engine::MaintenanceEngine;
use crate::intelligence::inhibition_engine::InhibitionEngine;
use crate::intelligence::correlation_engine::CorrelationEngine;

fn setup_db() -> Connection {
    let mut conn = Connection::open_in_memory().unwrap();
    run_migrations(&mut conn).unwrap();
    conn
}

#[test]
fn test_integration_1_telemetry_gap() {
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

    let maint_eng = MaintenanceEngine::new(&maint_repo);
    let corr_eng = CorrelationEngine::new(&inc_repo, &corr_repo);
    let inhib_eng = InhibitionEngine::new(&inc_repo, &corr_repo);
    
    let rule_eng = RuleEngine::new(&rule_repo, &tel_repo, &alert_repo, &inc_repo, &met_repo, &mon_repo);
    let notif_eng = NotificationEngine::new(&notif_repo, &route_repo, &maint_eng, &inhib_eng);

    // STEP 1: Prepare configuration
    let vm_id = Uuid::new_v4(); // VM 101

    let resource_id = Uuid::new_v4();
    res_repo.insert(&Resource {
        id: resource_id,
        vm_id,
        kind: "Service".to_string(),
        identifier: "nginx.service".to_string(),
        state: ResourceState::Monitored,
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
        deleted_at: None,
    }).unwrap();

    let monitor_id = Uuid::new_v4();
    conn.execute("INSERT INTO monitors (id, resource_id, state, interval_secs, collection_type, version, created_at, updated_at) VALUES (?1, ?2, 'ENABLED', 30, 'Systemd', 1, ?3, ?4)",
        rusqlite::params![monitor_id.to_string(), resource_id.to_string(), Utc::now().to_rfc3339(), Utc::now().to_rfc3339()]).unwrap();

    let metric_id = Uuid::new_v4();
    conn.execute("INSERT INTO metrics (id, monitor_id, name, value_type, unit) VALUES (?1, ?2, 'status', 'state', 'state')",
        rusqlite::params![metric_id.to_string(), monitor_id.to_string()]).unwrap();

    let rule_id = Uuid::new_v4();
    conn.execute("INSERT INTO rules (id, metric_id, state, operator, fire_value, fire_duration_secs, resolve_value, resolve_duration_secs, severity, version, created_at, updated_at) VALUES (?1, ?2, 'ENABLED', 'EQUAL', 'inactive', 120, 'active', 120, 'Critical', 1, ?3, ?4)",
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

    // STEP 2: Insert healthy telemetry
    let base_time = Utc.with_ymd_and_hms(2026, 8, 16, 17, 0, 0).unwrap();
    
    tel_repo.insert(&Telemetry {
        id: Uuid::new_v4(), metric_id, timestamp: base_time, value: None, string_value: Some("active".to_string()), observation: ObservationState::Healthy, labels: serde_json::json!({})
    }).unwrap();
    tel_repo.insert(&Telemetry {
        id: Uuid::new_v4(), metric_id, timestamp: base_time + Duration::seconds(30), value: None, string_value: Some("active".to_string()), observation: ObservationState::Healthy, labels: serde_json::json!({})
    }).unwrap();
    tel_repo.insert(&Telemetry {
        id: Uuid::new_v4(), metric_id, timestamp: base_time + Duration::seconds(60), value: None, string_value: Some("active".to_string()), observation: ObservationState::Healthy, labels: serde_json::json!({})
    }).unwrap();

    rule_eng.evaluate_all().unwrap();
    assert!(alert_repo.get_active_by_rule(rule_id).unwrap().is_none(), "Should not fire");

    // STEP 3: Simulate the failure
    for i in 0..4 {
        tel_repo.insert(&Telemetry {
            id: Uuid::new_v4(), metric_id, timestamp: base_time + Duration::seconds(90 + i * 30), value: None, string_value: Some("inactive".to_string()), observation: ObservationState::Problem, labels: serde_json::json!({})
        }).unwrap();
    }

    rule_eng.evaluate_all().unwrap();
    
    let active_alert = alert_repo.get_active_by_rule(rule_id).unwrap().expect("Alert should be firing");
    let active_inc = inc_repo.get_active_by_alert(active_alert.id).unwrap().expect("Incident should be open");

    // STEP 4: Verify routing
    notif_eng.process_incident(&active_inc, Some(rule_id), Some(vm_id), Some(resource_id), Some(metric_id)).unwrap();

    // Check notifications
    let mut stmt = conn.prepare("SELECT id, incident_id, route_id, channel_id FROM notifications").unwrap();
    let notifs: Vec<_> = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, String>(3)?))).unwrap().collect();
    assert_eq!(notifs.len(), 1, "Should have 1 notification");
    let notif = notifs[0].as_ref().unwrap();
    assert_eq!(notif.2, route_id.to_string(), "Route ID must match");
    assert_eq!(notif.3, channel_id.to_string(), "Channel ID must match");

    // STEP 5: The important telemetry-gap test
    for i in 0..4 {
        tel_repo.insert(&Telemetry {
            id: Uuid::new_v4(), metric_id, timestamp: base_time + Duration::seconds(210 + i * 30), value: None, string_value: None, observation: ObservationState::Unknown, labels: serde_json::json!({})
        }).unwrap();
    }

    rule_eng.evaluate_all().unwrap();

    // Incident should STILL be open
    let inc_after_gap = inc_repo.get_active_by_alert(active_alert.id).unwrap().expect("Incident should remain open during UNKNOWN gap");
    assert_eq!(inc_after_gap.id, active_inc.id);

    // Notification shouldn't duplicate
    let notifs_count: i64 = conn.query_row("SELECT COUNT(*) FROM notifications", [], |row| row.get(0)).unwrap();
    assert_eq!(notifs_count, 1, "Notification unchanged");

    // STEP 6: Simulate recovery
    for i in 0..4 {
        tel_repo.insert(&Telemetry {
            id: Uuid::new_v4(), metric_id, timestamp: base_time + Duration::seconds(330 + i * 30), value: None, string_value: Some("active".to_string()), observation: ObservationState::Healthy, labels: serde_json::json!({})
        }).unwrap();
    }

    rule_eng.evaluate_all().unwrap();
    
    let active_alert_after = alert_repo.get_active_by_rule(rule_id).unwrap();
    assert!(active_alert_after.is_none(), "Alert should be resolved");

    let inc_after_resolve = inc_repo.get_active_by_alert(active_alert.id).unwrap();
    assert!(inc_after_resolve.is_none(), "Incident should be resolved");

    // Incident 1 remains in history permanently
    let historical_inc: i64 = conn.query_row("SELECT COUNT(*) FROM incidents WHERE id = ?1", rusqlite::params![active_inc.id.to_string()], |row| row.get(0)).unwrap();
    assert_eq!(historical_inc, 1, "Incident must remain in history");
}
