use chrono::{DateTime, Utc};
use rusqlite::{Connection, OptionalExtension, Result, params};
use serde_json::Value;
use std::str::FromStr;
use uuid::Uuid;

use crate::domain::alert::{Alert, AlertState};
use crate::domain::discovery::{DiscoveryEvent, DiscoveryEventType};
use crate::domain::incident::{Incident, IncidentState};
use crate::domain::metric::{Metric, MetricValueType};
use crate::domain::monitor::{ConfigState, Monitor};
use crate::domain::resource::{Resource, ResourceState};
use crate::domain::rule::{Operator, Rule};
use crate::domain::telemetry::{ObservationState, Telemetry};

fn uuid_to_string(id: Uuid) -> String {
    id.to_string()
}
fn string_to_uuid(s: String) -> Uuid {
    Uuid::from_str(&s).unwrap_or_default()
}
fn dt_to_string(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339()
}
fn string_to_dt(s: String) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(&s)
        .map(|dt| dt.with_timezone(&Utc))
        .unwrap_or_else(|_| Utc::now())
}
fn opt_dt_to_string(dt: Option<DateTime<Utc>>) -> Option<String> {
    dt.map(dt_to_string)
}
fn opt_string_to_dt(s: Option<String>) -> Option<DateTime<Utc>> {
    s.map(string_to_dt)
}

pub fn resource_state_to_str(state: ResourceState) -> &'static str {
    match state {
        ResourceState::Discovered => "DISCOVERED",
        ResourceState::PendingUser => "PENDING_USER",
        ResourceState::Monitored => "MONITORED",
        ResourceState::Ignored => "IGNORED",
        ResourceState::Removed => "REMOVED",
    }
}
pub fn str_to_resource_state(s: &str) -> ResourceState {
    match s {
        "DISCOVERED" => ResourceState::Discovered,
        "PENDING_USER" => ResourceState::PendingUser,
        "MONITORED" => ResourceState::Monitored,
        "IGNORED" => ResourceState::Ignored,
        "REMOVED" => ResourceState::Removed,
        _ => ResourceState::Discovered,
    }
}

pub fn config_state_to_str(state: ConfigState) -> &'static str {
    match state {
        ConfigState::Enabled => "ENABLED",
        ConfigState::Disabled => "DISABLED",
    }
}
pub fn str_to_config_state(s: &str) -> ConfigState {
    match s {
        "ENABLED" => ConfigState::Enabled,
        "DISABLED" => ConfigState::Disabled,
        _ => ConfigState::Disabled,
    }
}

pub fn event_type_to_str(state: DiscoveryEventType) -> &'static str {
    match state {
        DiscoveryEventType::Discovered => "DISCOVERED",
        DiscoveryEventType::Changed => "CHANGED",
        DiscoveryEventType::Disappeared => "DISAPPEARED",
        DiscoveryEventType::Reappeared => "REAPPEARED",
    }
}

pub fn str_to_value_type(s: &str) -> MetricValueType {
    match s {
        "number" => MetricValueType::Number,
        "string" => MetricValueType::String,
        "state" => MetricValueType::State,
        _ => MetricValueType::Number,
    }
}
pub fn value_type_to_str(vt: MetricValueType) -> &'static str {
    match vt {
        MetricValueType::Number => "number",
        MetricValueType::String => "string",
        MetricValueType::State => "state",
    }
}

pub fn observation_to_str(obs: ObservationState) -> &'static str {
    match obs {
        ObservationState::Healthy => "HEALTHY",
        ObservationState::Problem => "PROBLEM",
        ObservationState::Unknown => "UNKNOWN",
    }
}
pub fn str_to_observation(s: &str) -> ObservationState {
    match s {
        "HEALTHY" => ObservationState::Healthy,
        "PROBLEM" => ObservationState::Problem,
        "UNKNOWN" => ObservationState::Unknown,
        _ => ObservationState::Unknown,
    }
}

pub fn operator_to_str(op: Operator) -> &'static str {
    match op {
        Operator::Equal => "EQUAL",
        Operator::NotEqual => "NOT_EQUAL",
        Operator::GreaterThan => "GREATER_THAN",
        Operator::LessThan => "LESS_THAN",
        Operator::GreaterOrEqual => "GREATER_OR_EQUAL",
        Operator::LessOrEqual => "LESS_OR_EQUAL",
    }
}
pub fn str_to_operator(s: &str) -> Operator {
    match s {
        "EQUAL" => Operator::Equal,
        "NOT_EQUAL" => Operator::NotEqual,
        "GREATER_THAN" => Operator::GreaterThan,
        "LESS_THAN" => Operator::LessThan,
        "GREATER_OR_EQUAL" => Operator::GreaterOrEqual,
        "LESS_OR_EQUAL" => Operator::LessOrEqual,
        _ => Operator::Equal,
    }
}

pub fn alert_state_to_str(s: AlertState) -> &'static str {
    match s {
        AlertState::Inactive => "INACTIVE",
        AlertState::Firing => "FIRING",
        AlertState::Resolved => "RESOLVED",
    }
}
pub fn str_to_alert_state(s: &str) -> AlertState {
    match s {
        "INACTIVE" => AlertState::Inactive,
        "FIRING" => AlertState::Firing,
        "RESOLVED" => AlertState::Resolved,
        _ => AlertState::Inactive,
    }
}

pub fn incident_state_to_str(s: IncidentState) -> &'static str {
    match s {
        IncidentState::Open => "OPEN",
        IncidentState::Acknowledged => "ACKNOWLEDGED",
        IncidentState::Resolved => "RESOLVED",
    }
}
pub fn str_to_incident_state(s: &str) -> IncidentState {
    match s {
        "OPEN" => IncidentState::Open,
        "ACKNOWLEDGED" => IncidentState::Acknowledged,
        "RESOLVED" => IncidentState::Resolved,
        _ => IncidentState::Open,
    }
}

pub struct ResourceRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> ResourceRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn insert(&self, resource: &Resource) -> Result<()> {
        self.conn.execute("INSERT INTO resources (id, vm_id, kind, identifier, state, version, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)", params![uuid_to_string(resource.id), uuid_to_string(resource.vm_id), resource.kind, resource.identifier, resource_state_to_str(resource.state), resource.version, dt_to_string(resource.created_at), dt_to_string(resource.updated_at), opt_dt_to_string(resource.deleted_at)])?;
        Ok(())
    }
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Resource>> {
        self.conn.query_row("SELECT id, vm_id, kind, identifier, state, version, created_at, updated_at, deleted_at FROM resources WHERE id = ?1", params![uuid_to_string(id)], |row| Ok(Resource { id: string_to_uuid(row.get(0)?), vm_id: string_to_uuid(row.get(1)?), kind: row.get(2)?, identifier: row.get(3)?, state: str_to_resource_state(&row.get::<_, String>(4)?), version: row.get(5)?, created_at: string_to_dt(row.get(6)?), updated_at: string_to_dt(row.get(7)?), deleted_at: opt_string_to_dt(row.get(8)?) })).optional()
    }
    pub fn list_by_vm_and_kind(&self, vm_id: Uuid, kind: &str) -> Result<Vec<Resource>> {
        let mut stmt = self.conn.prepare("SELECT id, vm_id, kind, identifier, state, version, created_at, updated_at, deleted_at FROM resources WHERE vm_id = ?1 AND kind = ?2 AND deleted_at IS NULL")?;
        let rows = stmt.query_map(params![uuid_to_string(vm_id), kind], |row| {
            Ok(Resource {
                id: string_to_uuid(row.get(0)?),
                vm_id: string_to_uuid(row.get(1)?),
                kind: row.get(2)?,
                identifier: row.get(3)?,
                state: str_to_resource_state(&row.get::<_, String>(4)?),
                version: row.get(5)?,
                created_at: string_to_dt(row.get(6)?),
                updated_at: string_to_dt(row.get(7)?),
                deleted_at: opt_string_to_dt(row.get(8)?),
            })
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
    pub fn update_state(&self, id: Uuid, state: ResourceState) -> Result<()> {
        self.conn.execute(
            "UPDATE resources SET state = ?1, updated_at = ?2, version = version + 1 WHERE id = ?3",
            params![
                resource_state_to_str(state),
                dt_to_string(Utc::now()),
                uuid_to_string(id)
            ],
        )?;
        Ok(())
    }
}

pub struct MonitorRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> MonitorRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn get_by_resource_id(&self, resource_id: Uuid) -> Result<Vec<Monitor>> {
        let mut stmt = self.conn.prepare("SELECT id, resource_id, state, interval_secs, collection_type, version, created_at, updated_at, deleted_at FROM monitors WHERE resource_id = ?1 AND deleted_at IS NULL")?;
        let rows = stmt.query_map(params![uuid_to_string(resource_id)], |row| {
            Ok(Monitor {
                id: string_to_uuid(row.get(0)?),
                resource_id: string_to_uuid(row.get(1)?),
                state: str_to_config_state(&row.get::<_, String>(2)?),
                interval_secs: row.get(3)?,
                collection_type: row.get(4)?,
                version: row.get(5)?,
                created_at: string_to_dt(row.get(6)?),
                updated_at: string_to_dt(row.get(7)?),
                deleted_at: opt_string_to_dt(row.get(8)?),
            })
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

pub struct MetricRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> MetricRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn get_by_monitor_and_name(&self, monitor_id: Uuid, name: &str) -> Result<Option<Metric>> {
        self.conn.query_row("SELECT id, monitor_id, name, value_type, unit FROM metrics WHERE monitor_id = ?1 AND name = ?2", params![uuid_to_string(monitor_id), name], |row| Ok(Metric { id: string_to_uuid(row.get(0)?), monitor_id: string_to_uuid(row.get(1)?), name: row.get(2)?, value_type: str_to_value_type(&row.get::<_, String>(3)?), unit: row.get(4)? })).optional()
    }
}

pub struct TelemetryRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> TelemetryRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn insert(&self, telemetry: &Telemetry) -> Result<()> {
        self.conn.execute("INSERT INTO telemetry (id, metric_id, timestamp, value, string_value, observation, labels) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)", params![uuid_to_string(telemetry.id), uuid_to_string(telemetry.metric_id), dt_to_string(telemetry.timestamp), telemetry.value, telemetry.string_value, observation_to_str(telemetry.observation), telemetry.labels])?;
        Ok(())
    }

    // In reality, this would query a time-window of telemetry data for the rule engine evaluator
    pub fn get_recent_for_metric(
        &self,
        metric_id: Uuid,
        since: DateTime<Utc>,
    ) -> Result<Vec<Telemetry>> {
        let mut stmt = self.conn.prepare("SELECT id, metric_id, timestamp, value, string_value, observation, labels FROM telemetry WHERE metric_id = ?1 AND timestamp >= ?2 ORDER BY timestamp DESC")?;
        let rows = stmt.query_map(
            params![uuid_to_string(metric_id), dt_to_string(since)],
            |row| {
                Ok(Telemetry {
                    id: string_to_uuid(row.get(0)?),
                    metric_id: string_to_uuid(row.get(1)?),
                    timestamp: string_to_dt(row.get(2)?),
                    value: row.get(3)?,
                    string_value: row.get(4)?,
                    observation: str_to_observation(&row.get::<_, String>(5)?),
                    labels: row.get(6)?,
                })
            },
        )?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

pub struct DiscoveryEventRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> DiscoveryEventRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn insert(&self, event: &DiscoveryEvent) -> Result<()> {
        self.conn.execute("INSERT INTO discovery_events (id, vm_id, resource_id, event_type, discovered_at, summary) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid_to_string(event.id), uuid_to_string(event.vm_id), event.resource_id.map(uuid_to_string), event_type_to_str(event.event_type), dt_to_string(event.discovered_at), event.summary])?;
        Ok(())
    }
}

pub struct RuleRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> RuleRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn list_enabled(&self) -> Result<Vec<Rule>> {
        let mut stmt = self.conn.prepare("SELECT id, metric_id, state, operator, fire_value, fire_duration_secs, resolve_value, resolve_duration_secs, severity, version, created_at, updated_at, deleted_at FROM rules WHERE state = 'ENABLED' AND deleted_at IS NULL")?;
        let rows = stmt.query_map([], |row| {
            Ok(Rule {
                id: string_to_uuid(row.get(0)?),
                metric_id: string_to_uuid(row.get(1)?),
                state: str_to_config_state(&row.get::<_, String>(2)?),
                operator: str_to_operator(&row.get::<_, String>(3)?),
                fire_value: row.get(4)?,
                fire_duration_secs: row.get(5)?,
                resolve_value: row.get(6)?,
                resolve_duration_secs: row.get(7)?,
                severity: row.get(8)?,
                version: row.get(9)?,
                created_at: string_to_dt(row.get(10)?),
                updated_at: string_to_dt(row.get(11)?),
                deleted_at: opt_string_to_dt(row.get(12)?),
            })
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

pub struct AlertRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> AlertRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn insert(&self, alert: &Alert) -> Result<()> {
        self.conn.execute("INSERT INTO alerts (id, rule_id, state, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5)", params![uuid_to_string(alert.id), uuid_to_string(alert.rule_id), alert_state_to_str(alert.state), dt_to_string(alert.created_at), dt_to_string(alert.updated_at)])?;
        Ok(())
    }
    pub fn update_state(&self, id: Uuid, state: AlertState) -> Result<()> {
        self.conn.execute(
            "UPDATE alerts SET state = ?1, updated_at = ?2 WHERE id = ?3",
            params![
                alert_state_to_str(state),
                dt_to_string(Utc::now()),
                uuid_to_string(id)
            ],
        )?;
        Ok(())
    }
    pub fn get_active_by_rule(&self, rule_id: Uuid) -> Result<Option<Alert>> {
        self.conn.query_row("SELECT id, rule_id, state, created_at, updated_at FROM alerts WHERE rule_id = ?1 AND state = 'FIRING'", params![uuid_to_string(rule_id)], |row| Ok(Alert { id: string_to_uuid(row.get(0)?), rule_id: string_to_uuid(row.get(1)?), state: str_to_alert_state(&row.get::<_, String>(2)?), created_at: string_to_dt(row.get(3)?), updated_at: string_to_dt(row.get(4)?) })).optional()
    }
}

pub struct IncidentRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> IncidentRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }
    pub fn insert(&self, incident: &Incident) -> Result<()> {
        self.conn.execute("INSERT INTO incidents (id, alert_id, state, severity, created_at, resolved_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid_to_string(incident.id), uuid_to_string(incident.alert_id), incident_state_to_str(incident.state), incident.severity, dt_to_string(incident.created_at), opt_dt_to_string(incident.resolved_at)])?;
        Ok(())
    }
    pub fn update_state(&self, id: Uuid, state: IncidentState) -> Result<()> {
        let resolved_at = if state == IncidentState::Resolved {
            Some(dt_to_string(Utc::now()))
        } else {
            None
        };
        self.conn.execute("UPDATE incidents SET state = ?1, resolved_at = COALESCE(?2, resolved_at) WHERE id = ?3", params![incident_state_to_str(state), resolved_at, uuid_to_string(id)])?;
        Ok(())
    }
    pub fn get_active_by_alert(&self, alert_id: Uuid) -> Result<Option<Incident>> {
        self.conn.query_row("SELECT id, alert_id, state, severity, created_at, resolved_at FROM incidents WHERE alert_id = ?1 AND state != 'RESOLVED'", params![uuid_to_string(alert_id)], |row| Ok(Incident { id: string_to_uuid(row.get(0)?), alert_id: string_to_uuid(row.get(1)?), state: str_to_incident_state(&row.get::<_, String>(2)?), severity: row.get(3)?, created_at: string_to_dt(row.get(4)?), resolved_at: opt_string_to_dt(row.get(5)?) })).optional()
    }
}
use crate::domain::maintenance::{MaintenanceScopeType, MaintenanceWindow};

pub fn scope_type_to_str(s: MaintenanceScopeType) -> &'static str {
    match s {
        MaintenanceScopeType::Global => "GLOBAL",
        MaintenanceScopeType::Vm => "VM",
        MaintenanceScopeType::Resource => "RESOURCE",
        MaintenanceScopeType::Rule => "RULE",
    }
}
pub fn str_to_scope_type(s: &str) -> MaintenanceScopeType {
    match s {
        "GLOBAL" => MaintenanceScopeType::Global,
        "VM" => MaintenanceScopeType::Vm,
        "RESOURCE" => MaintenanceScopeType::Resource,
        "RULE" => MaintenanceScopeType::Rule,
        _ => MaintenanceScopeType::Global,
    }
}

pub struct MaintenanceWindowRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> MaintenanceWindowRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, window: &MaintenanceWindow) -> Result<()> {
        self.conn.execute("INSERT INTO maintenance_windows (id, scope_type, scope_id, start_time, end_time, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid_to_string(window.id), scope_type_to_str(window.scope_type), window.scope_id.map(uuid_to_string), dt_to_string(window.start_time), dt_to_string(window.end_time), window.created_by])?;
        Ok(())
    }

    pub fn get_active(&self) -> Result<Vec<MaintenanceWindow>> {
        let now = dt_to_string(Utc::now());
        let mut stmt = self.conn.prepare("SELECT id, scope_type, scope_id, start_time, end_time, created_by FROM maintenance_windows WHERE start_time <= ?1 AND end_time >= ?1")?;
        let rows = stmt.query_map(params![now], |row| {
            Ok(MaintenanceWindow {
                id: string_to_uuid(row.get(0)?),
                scope_type: str_to_scope_type(&row.get::<_, String>(1)?),
                scope_id: opt_string_to_uuid(row.get(2)?),
                start_time: string_to_dt(row.get(3)?),
                end_time: string_to_dt(row.get(4)?),
                created_by: row.get(5)?,
            })
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

fn opt_string_to_uuid(s: Option<String>) -> Option<Uuid> {
    s.map(string_to_uuid)
}

// Additional helper queries for tracing relationships
impl<'a> AlertRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Alert>> {
        self.conn
            .query_row(
                "SELECT id, rule_id, state, created_at, updated_at FROM alerts WHERE id = ?1",
                params![uuid_to_string(id)],
                |row| {
                    Ok(Alert {
                        id: string_to_uuid(row.get(0)?),
                        rule_id: string_to_uuid(row.get(1)?),
                        state: str_to_alert_state(&row.get::<_, String>(2)?),
                        created_at: string_to_dt(row.get(3)?),
                        updated_at: string_to_dt(row.get(4)?),
                    })
                },
            )
            .optional()
    }
}

impl<'a> RuleRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Rule>> {
        self.conn.query_row("SELECT id, metric_id, state, operator, fire_value, fire_duration_secs, resolve_value, resolve_duration_secs, severity, version, created_at, updated_at, deleted_at FROM rules WHERE id = ?1", params![uuid_to_string(id)], |row| Ok(Rule { id: string_to_uuid(row.get(0)?), metric_id: string_to_uuid(row.get(1)?), state: str_to_config_state(&row.get::<_, String>(2)?), operator: str_to_operator(&row.get::<_, String>(3)?), fire_value: row.get(4)?, fire_duration_secs: row.get(5)?, resolve_value: row.get(6)?, resolve_duration_secs: row.get(7)?, severity: row.get(8)?, version: row.get(9)?, created_at: string_to_dt(row.get(10)?), updated_at: string_to_dt(row.get(11)?), deleted_at: opt_string_to_dt(row.get(12)?) })).optional()
    }
}

impl<'a> MetricRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Metric>> {
        self.conn
            .query_row(
                "SELECT id, monitor_id, name, value_type, unit FROM metrics WHERE id = ?1",
                params![uuid_to_string(id)],
                |row| {
                    Ok(Metric {
                        id: string_to_uuid(row.get(0)?),
                        monitor_id: string_to_uuid(row.get(1)?),
                        name: row.get(2)?,
                        value_type: str_to_value_type(&row.get::<_, String>(3)?),
                        unit: row.get(4)?,
                    })
                },
            )
            .optional()
    }
}

impl<'a> MonitorRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Monitor>> {
        self.conn.query_row("SELECT id, resource_id, state, interval_secs, collection_type, version, created_at, updated_at, deleted_at FROM monitors WHERE id = ?1", params![uuid_to_string(id)], |row| Ok(Monitor { id: string_to_uuid(row.get(0)?), resource_id: string_to_uuid(row.get(1)?), state: str_to_config_state(&row.get::<_, String>(2)?), interval_secs: row.get(3)?, collection_type: row.get(4)?, version: row.get(5)?, created_at: string_to_dt(row.get(6)?), updated_at: string_to_dt(row.get(7)?), deleted_at: opt_string_to_dt(row.get(8)?) })).optional()
    }
}

use crate::domain::notification::Notification;

pub struct NotificationRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> NotificationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, notif: &Notification) -> Result<()> {
        self.conn.execute("INSERT INTO notifications (id, incident_id, channel_id, sent_at, success, error_message) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid_to_string(notif.id), uuid_to_string(notif.incident_id), uuid_to_string(notif.channel_id), dt_to_string(notif.sent_at), notif.success, notif.error_message])?;
        Ok(())
    }
}

use rusqlite::Result;
use std::collections::HashMap;

impl<'a> TelemetryRepository<'a> {
    pub fn get_latest_for_metric(&self, metric_id: Uuid) -> Result<Option<Telemetry>> {
        self.conn
            .query_row(
                "SELECT id, metric_id, timestamp, value, string_value, observation, labels 
             FROM telemetry WHERE metric_id = ?1 ORDER BY timestamp DESC LIMIT 1",
                params![uuid_to_string(metric_id)],
                |row| {
                    Ok(Telemetry {
                        id: string_to_uuid(row.get(0)?),
                        metric_id: string_to_uuid(row.get(1)?),
                        timestamp: string_to_dt(row.get(2)?),
                        value: row.get(3)?,
                        string_value: row.get(4)?,
                        observation: str_to_observation(&row.get::<_, String>(5)?),
                        labels: row.get(6)?,
                    })
                },
            )
            .optional()
    }
}

pub struct ExporterQueries<'a> {
    pub conn: &'a Connection,
}
impl<'a> ExporterQueries<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn get_monitored_metrics(&self) -> Result<Vec<(Uuid, Uuid, String, String, String)>> {
        // Returns: metric_id, vm_id, resource_kind, resource_identifier, metric_name
        let mut stmt = self.conn.prepare("
            SELECT m.id, r.vm_id, r.kind, r.identifier, m.name 
            FROM metrics m
            JOIN monitors mo ON m.monitor_id = mo.id
            JOIN resources r ON mo.resource_id = r.id
            WHERE r.state = 'MONITORED' AND mo.state = 'ENABLED' AND mo.deleted_at IS NULL AND r.deleted_at IS NULL
        ")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                string_to_uuid(row.get(0)?),
                string_to_uuid(row.get(1)?),
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

impl<'a> ExporterQueries<'a> {
    pub fn get_monitored_metrics_with_vm(
        &self,
    ) -> Result<Vec<(Uuid, u32, String, String, String, String)>> {
        // Returns: metric_id, proxmox_vmid, vm_name, resource_kind, resource_identifier, metric_name
        let mut stmt = self.conn.prepare("
            SELECT m.id, v.proxmox_vmid, v.name, r.kind, r.identifier, m.name 
            FROM metrics m
            JOIN monitors mo ON m.monitor_id = mo.id
            JOIN resources r ON mo.resource_id = r.id
            JOIN vms v ON r.vm_id = v.id
            WHERE r.state = 'MONITORED' AND mo.state = 'ENABLED' AND mo.deleted_at IS NULL AND r.deleted_at IS NULL
        ")?;
        let rows = stmt.query_map([], |row| {
            Ok((
                string_to_uuid(row.get(0)?),
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
                row.get(5)?,
            ))
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

use crate::domain::incident::{CorrelationType, IncidentCorrelation};

pub fn str_to_correlation_type(s: &str) -> CorrelationType {
    match s {
        "NODE_TO_VM" => CorrelationType::NodeToVm,
        "VM_TO_RESOURCE" => CorrelationType::VmToResource,
        "NETWORK_TO_RESOURCE" => CorrelationType::NetworkToResource,
        "GUEST_AGENT_TO_RESOURCE" => CorrelationType::GuestAgentToResource,
        "TEMPORAL" => CorrelationType::Temporal,
        _ => CorrelationType::Temporal,
    }
}

pub fn correlation_type_to_str(c: &CorrelationType) -> &'static str {
    match c {
        CorrelationType::NodeToVm => "NODE_TO_VM",
        CorrelationType::VmToResource => "VM_TO_RESOURCE",
        CorrelationType::NetworkToResource => "NETWORK_TO_RESOURCE",
        CorrelationType::GuestAgentToResource => "GUEST_AGENT_TO_RESOURCE",
        CorrelationType::Temporal => "TEMPORAL",
    }
}

pub struct IncidentCorrelationRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> IncidentCorrelationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(&self, corr: &IncidentCorrelation) -> Result<()> {
        self.conn.execute("INSERT INTO incident_correlations (id, parent_incident_id, child_incident_id, correlation_type, confidence_score, reason, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![uuid_to_string(corr.id), uuid_to_string(corr.parent_incident_id), uuid_to_string(corr.child_incident_id), correlation_type_to_str(&corr.correlation_type), corr.confidence_score, corr.reason, dt_to_string(corr.created_at)])?;
        Ok(())
    }

    pub fn get_by_child_id(&self, child_id: Uuid) -> Result<Option<IncidentCorrelation>> {
        self.conn.query_row("SELECT id, parent_incident_id, child_incident_id, correlation_type, confidence_score, reason, created_at FROM incident_correlations WHERE child_incident_id = ?1 LIMIT 1",
            params![uuid_to_string(child_id)],
            |row| Ok(IncidentCorrelation {
                id: string_to_uuid(row.get(0)?),
                parent_incident_id: string_to_uuid(row.get(1)?),
                child_incident_id: string_to_uuid(row.get(2)?),
                correlation_type: str_to_correlation_type(&row.get::<_, String>(3)?),
                confidence_score: row.get(4)?,
                reason: row.get(5)?,
                created_at: string_to_dt(row.get(6)?)
            })).optional()
    }
}

impl<'a> IncidentRepository<'a> {
    pub fn list_open_incidents(&self) -> Result<Vec<Incident>> {
        let mut stmt = self.conn.prepare("SELECT id, alert_id, state, severity, created_at, resolved_at FROM incidents WHERE state = 'OPEN' OR state = 'ACKNOWLEDGED'")?;
        let rows = stmt.query_map([], |row| {
            Ok(Incident {
                id: string_to_uuid(row.get(0)?),
                alert_id: string_to_uuid(row.get(1)?),
                state: str_to_incident_state(&row.get::<_, String>(2)?),
                severity: row.get(3)?,
                created_at: string_to_dt(row.get(4)?),
                resolved_at: opt_string_to_dt(row.get(5)?),
            })
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}

impl<'a> ResourceRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<crate::domain::resource::Resource>> {
        self.conn.query_row("SELECT id, vm_id, kind, identifier, state, created_at, updated_at, deleted_at FROM resources WHERE id = ?1", params![uuid_to_string(id)], |row| {
            Ok(crate::domain::resource::Resource {
                id: string_to_uuid(row.get(0)?),
                vm_id: string_to_uuid(row.get(1)?),
                kind: row.get(2)?,
                identifier: row.get(3)?,
                state: str_to_resource_state(&row.get::<_, String>(4)?),
                created_at: string_to_dt(row.get(5)?),
                updated_at: string_to_dt(row.get(6)?),
                deleted_at: opt_string_to_dt(row.get(7)?)
            })
        }).optional()
    }
}

pub struct NotificationRouteRepository<'a> {
    pub conn: &'a Connection,
}
impl<'a> NotificationRouteRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    pub fn insert(
        &self,
        route: &crate::domain::notification::NotificationRoute,
    ) -> rusqlite::Result<()> {
        self.conn.execute("INSERT INTO notification_routes (id, name, rule_id, severity, scope_type, scope_id, priority, template_id, channel_id, state, version, created_at, updated_at, deleted_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
            rusqlite::params![uuid_to_string(route.id), route.name, route.rule_id.map(uuid_to_string), route.severity, route.scope_type, route.scope_id.map(uuid_to_string), route.priority, uuid_to_string(route.template_id), uuid_to_string(route.channel_id), config_state_to_str(route.state), route.version, dt_to_string(route.created_at), dt_to_string(route.updated_at), opt_dt_to_string(route.deleted_at)])?;
        Ok(())
    }

    pub fn list_active(
        &self,
    ) -> rusqlite::Result<Vec<crate::domain::notification::NotificationRoute>> {
        let mut stmt = self.conn.prepare("SELECT id, name, rule_id, severity, scope_type, scope_id, priority, template_id, channel_id, state, version, created_at, updated_at, deleted_at FROM notification_routes WHERE state = 'ENABLED' AND deleted_at IS NULL ORDER BY priority DESC")?;
        let rows = stmt.query_map([], |row| {
            Ok(crate::domain::notification::NotificationRoute {
                id: string_to_uuid(row.get(0)?),
                name: row.get(1)?,
                rule_id: row.get::<_, Option<String>>(2)?.map(string_to_uuid),
                severity: row.get(3)?,
                scope_type: row.get(4)?,
                scope_id: row.get::<_, Option<String>>(5)?.map(string_to_uuid),
                priority: row.get(6)?,
                template_id: string_to_uuid(row.get(7)?),
                channel_id: string_to_uuid(row.get(8)?),
                state: str_to_config_state(&row.get::<_, String>(9)?),
                version: row.get(10)?,
                created_at: string_to_dt(row.get(11)?),
                updated_at: string_to_dt(row.get(12)?),
                deleted_at: opt_string_to_dt(row.get(13)?),
            })
        })?;
        let mut results = Vec::new();
        for row in rows {
            results.push(row?);
        }
        Ok(results)
    }
}
