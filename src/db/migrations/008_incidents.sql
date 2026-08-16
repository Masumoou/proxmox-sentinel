CREATE TABLE incidents (
    id TEXT PRIMARY KEY,
    alert_id TEXT NOT NULL REFERENCES alerts(id),
    vm_id TEXT NOT NULL REFERENCES vms(id),
    state TEXT NOT NULL,
    started_at DATETIME NOT NULL,
    acknowledged_at DATETIME,
    resolved_at DATETIME,
    acknowledged_by TEXT,
    root_cause_summary TEXT
);
CREATE INDEX idx_incidents_vm_id_state ON incidents(vm_id, state);


