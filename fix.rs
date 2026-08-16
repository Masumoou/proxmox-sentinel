use crate::domain::maintenance::{MaintenanceWindow, MaintenanceScopeType};

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

pub struct MaintenanceWindowRepository<'a> { pub conn: &'a Connection }
impl<'a> MaintenanceWindowRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    
    pub fn insert(&self, window: &MaintenanceWindow) -> Result<()> {
        self.conn.execute("INSERT INTO maintenance_windows (id, scope_type, scope_id, start_time, end_time, created_by) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid_to_string(window.id), scope_type_to_str(window.scope_type), window.scope_id.map(uuid_to_string), dt_to_string(window.start_time), dt_to_string(window.end_time), window.created_by])?; Ok(())
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
        let mut results = Vec::new(); for row in rows { results.push(row?); } Ok(results)
    }
}

fn opt_string_to_uuid(s: Option<String>) -> Option<Uuid> { s.map(string_to_uuid) }

// Additional helper queries for tracing relationships
impl<'a> AlertRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Alert>> {
        self.conn.query_row("SELECT id, rule_id, state, created_at, updated_at FROM alerts WHERE id = ?1", params![uuid_to_string(id)], |row| Ok(Alert { id: string_to_uuid(row.get(0)?), rule_id: string_to_uuid(row.get(1)?), state: str_to_alert_state(&row.get::<_, String>(2)?), created_at: string_to_dt(row.get(3)?), updated_at: string_to_dt(row.get(4)?) })).optional()
    }
}

impl<'a> RuleRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Rule>> {
        self.conn.query_row("SELECT id, metric_id, state, operator, fire_value, fire_duration_secs, resolve_value, resolve_duration_secs, severity, version, created_at, updated_at, deleted_at FROM rules WHERE id = ?1", params![uuid_to_string(id)], |row| Ok(Rule { id: string_to_uuid(row.get(0)?), metric_id: string_to_uuid(row.get(1)?), state: str_to_config_state(&row.get::<_, String>(2)?), operator: str_to_operator(&row.get::<_, String>(3)?), fire_value: row.get(4)?, fire_duration_secs: row.get(5)?, resolve_value: row.get(6)?, resolve_duration_secs: row.get(7)?, severity: row.get(8)?, version: row.get(9)?, created_at: string_to_dt(row.get(10)?), updated_at: string_to_dt(row.get(11)?), deleted_at: opt_string_to_dt(row.get(12)?) })).optional()
    }
}

impl<'a> MetricRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Metric>> {
        self.conn.query_row("SELECT id, monitor_id, name, value_type, unit FROM metrics WHERE id = ?1", params![uuid_to_string(id)], |row| Ok(Metric { id: string_to_uuid(row.get(0)?), monitor_id: string_to_uuid(row.get(1)?), name: row.get(2)?, value_type: str_to_value_type(&row.get::<_, String>(3)?), unit: row.get(4)? })).optional()
    }
}

impl<'a> MonitorRepository<'a> {
    pub fn get_by_id(&self, id: Uuid) -> Result<Option<Monitor>> {
        self.conn.query_row("SELECT id, resource_id, state, interval_secs, collection_type, version, created_at, updated_at, deleted_at FROM monitors WHERE id = ?1", params![uuid_to_string(id)], |row| Ok(Monitor { id: string_to_uuid(row.get(0)?), resource_id: string_to_uuid(row.get(1)?), state: str_to_config_state(&row.get::<_, String>(2)?), interval_secs: row.get(3)?, collection_type: row.get(4)?, version: row.get(5)?, created_at: string_to_dt(row.get(6)?), updated_at: string_to_dt(row.get(7)?), deleted_at: opt_string_to_dt(row.get(8)?) })).optional()
    }
}

use crate::domain::notification::Notification;

pub struct NotificationRepository<'a> { pub conn: &'a Connection }
impl<'a> NotificationRepository<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }
    
    pub fn insert(&self, notif: &Notification) -> Result<()> {
        self.conn.execute("INSERT INTO notifications (id, incident_id, channel_id, sent_at, success, error_message) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![uuid_to_string(notif.id), uuid_to_string(notif.incident_id), uuid_to_string(notif.channel_id), dt_to_string(notif.sent_at), notif.success, notif.error_message])?; Ok(())
    }
}
use rusqlite::Result;
use std::collections::HashMap;

impl<'a> TelemetryRepository<'a> {
    pub fn get_latest_for_metric(&self, metric_id: Uuid) -> Result<Option<Telemetry>> {
        self.conn.query_row(
            "SELECT id, metric_id, timestamp, value, string_value, observation, labels 
             FROM telemetry WHERE metric_id = ?1 ORDER BY timestamp DESC LIMIT 1",
            params![uuid_to_string(metric_id)],
            |row| Ok(Telemetry {
                id: string_to_uuid(row.get(0)?),
                metric_id: string_to_uuid(row.get(1)?),
                timestamp: string_to_dt(row.get(2)?),
                value: row.get(3)?,
                string_value: row.get(4)?,
                observation: str_to_observation(&row.get::<_, String>(5)?),
                labels: row.get(6)?
            })
        ).optional()
    }
}
pub struct ExporterQueries<'a> { pub conn: &'a Connection }
impl<'a> ExporterQueries<'a> {
    pub fn new(conn: &'a Connection) -> Self { Self { conn } }

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
        let mut results = Vec::new(); for row in rows { results.push(row?); } Ok(results)
    }
}
impl<'a> ExporterQueries<'a> {
    pub fn get_monitored_metrics_with_vm(&self) -> Result<Vec<(Uuid, u32, String, String, String, String)>> {
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
        let mut results = Vec::new(); for row in rows { results.push(row?); } Ok(results)
    }
}

 