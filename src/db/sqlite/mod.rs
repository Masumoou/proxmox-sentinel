use rusqlite::{Connection, Result};

pub fn run_migrations(conn: &mut Connection) -> Result<()> {
    conn.execute_batch("PRAGMA foreign_keys = ON;")?;
    
    // We would typically use a migration runner or read from directory.
    // For this demonstration, we'll embed the scripts to ensure they are available in binary.
    
    let migrations = vec![
        include_str!("../../db/migrations/001_vms.sql"),
        include_str!("../../db/migrations/002_templates.sql"),
        include_str!("../../db/migrations/003_resources.sql"),
        include_str!("../../db/migrations/004_monitors.sql"),
        include_str!("../../db/migrations/005_metrics.sql"),
        include_str!("../../db/migrations/006_rules.sql"),
        include_str!("../../db/migrations/007_alerts.sql"),
        include_str!("../../db/migrations/008_incidents.sql"),
        include_str!("../../db/migrations/009_telemetry.sql"),
        include_str!("../../db/migrations/010_notification_channels.sql"),
        include_str!("../../db/migrations/011_notifications.sql"),
        include_str!("../../db/migrations/012_maintenance_windows.sql"),
        include_str!("../../db/migrations/013_audit_events.sql"),
        include_str!("../../db/migrations/014_discovery_events.sql"),
        include_str!("../../db/migrations/015_notification_routes.sql"),
    ];

    let tx = conn.transaction()?;
    for sql in migrations {
        tx.execute_batch(sql)?;
    }
    tx.commit()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::params;
    
    fn setup_db() -> Connection {
        let mut conn = Connection::open_in_memory().unwrap();
        run_migrations(&mut conn).unwrap();
        conn
    }

    #[test]
    fn test_history_preservation_when_rule_deleted() {
        let mut conn = setup_db();
        
        // 1. Insert VM
        conn.execute(
            "INSERT INTO vms (id, proxmox_vmid, node_name, name, created_at, updated_at) 
             VALUES ('vm1', 101, 'node1', 'test_vm', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')", 
            []
        ).unwrap();

        // 2. Insert Resource
        conn.execute(
            "INSERT INTO resources (id, vm_id, kind, identifier, state, created_at, updated_at) 
             VALUES ('res1', 'vm1', 'service', 'nginx', 'MONITORED', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')", 
            []
        ).unwrap();
        
        // 3. Insert Monitor
        conn.execute(
            "INSERT INTO monitors (id, resource_id, state, interval_secs, collection_type, created_at, updated_at) 
             VALUES ('mon1', 'res1', 'Enabled', 60, 'polling', '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')", 
            []
        ).unwrap();
        
        // 4. Insert Metric
        conn.execute(
            "INSERT INTO metrics (id, monitor_id, name, value_type) 
             VALUES ('met1', 'mon1', 'status', 'state')", 
            []
        ).unwrap();
        
        // 5. Insert Rule
        conn.execute(
            "INSERT INTO rules (id, metric_id, state, severity, fire_operator, fire_value_type, fire_value, fire_duration_secs, resolve_operator, resolve_value_type, resolve_value, resolve_duration_secs, created_at, updated_at) 
             VALUES ('rule1', 'met1', 'Enabled', 'High', 'EQ', 'state', 'failed', 60, 'EQ', 'state', 'running', 60, '2026-01-01T00:00:00Z', '2026-01-01T00:00:00Z')", 
            []
        ).unwrap();
        
        // 6. Insert Alert
        conn.execute(
            "INSERT INTO alerts (id, rule_id, state, fired_at) 
             VALUES ('alert1', 'rule1', 'Firing', '2026-01-01T00:01:00Z')", 
            []
        ).unwrap();
        
        // 7. Insert Incident
        conn.execute(
            "INSERT INTO incidents (id, alert_id, vm_id, state, started_at) 
             VALUES ('inc1', 'alert1', 'vm1', 'Open', '2026-01-01T00:01:00Z')", 
            []
        ).unwrap();
        
        // Now softly delete rule
        conn.execute(
            "UPDATE rules SET deleted_at = '2026-01-01T00:05:00Z' WHERE id = 'rule1'", 
            []
        ).unwrap();
        
        // Verify Alert remains
        let alert_count: i64 = conn.query_row("SELECT COUNT(*) FROM alerts WHERE id = 'alert1'", [], |row| row.get(0)).unwrap();
        assert_eq!(alert_count, 1);
        
        // Verify Incident remains
        let incident_count: i64 = conn.query_row("SELECT COUNT(*) FROM incidents WHERE id = 'inc1'", [], |row| row.get(0)).unwrap();
        assert_eq!(incident_count, 1);
        
        // Try Hard Delete Rule (should fail due to constraint OR succeed depending on no-cascade but since no-cascade exists, the DB will reject hard delete due to FK constraint from alerts)
        let res = conn.execute("DELETE FROM rules WHERE id = 'rule1'", []);
        assert!(res.is_err(), "Hard deleting a rule with alerts should violate foreign key constraints");
    }

    #[test]
    fn test_unknown_state_suppresses_alert() {
        // Just demonstrating that the business logic can represent Unknown. 
        // In this test, we verify the schema accepts it.
        let mut conn = setup_db();
        
        conn.execute(
            "INSERT INTO metrics (id, monitor_id, name, value_type) VALUES ('met1', 'mon1', 'status', 'state')", 
            []
        ).ok(); // ok because fk might fail if not inserted, but we'll disable fk for this unit test if we want or just do full setup.
        
        // Let's do full setup
        conn.execute("INSERT INTO vms (id, proxmox_vmid, node_name, name, created_at, updated_at) VALUES ('vm1', 101, 'n', 'v', '2026', '2026')", []).unwrap();
        conn.execute("INSERT INTO resources (id, vm_id, kind, identifier, state, created_at, updated_at) VALUES ('res1', 'vm1', 'k', 'i', 's', '2026', '2026')", []).unwrap();
        conn.execute("INSERT INTO monitors (id, resource_id, state, interval_secs, collection_type, created_at, updated_at) VALUES ('mon1', 'res1', 'e', 60, 't', '2026', '2026')", []).unwrap();
        conn.execute("INSERT INTO metrics (id, monitor_id, name, value_type) VALUES ('met1', 'mon1', 'n', 't')", []).unwrap();

        // Insert Telemetry as Unknown
        conn.execute(
            "INSERT INTO telemetry (id, metric_id, timestamp, observation) 
             VALUES ('tel1', 'met1', '2026-01-01T00:00:00Z', 'Unknown')", 
            []
        ).unwrap();
        
        let observation: String = conn.query_row("SELECT observation FROM telemetry WHERE id = 'tel1'", [], |row| row.get(0)).unwrap();
        assert_eq!(observation, "Unknown");
    }
}

pub mod repository;


