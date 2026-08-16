CREATE TABLE discovery_events (
    id TEXT PRIMARY KEY,
    vm_id TEXT NOT NULL REFERENCES vms(id),
    resource_id TEXT,
    event_type TEXT NOT NULL,
    discovered_at DATETIME NOT NULL,
    summary TEXT NOT NULL
);
CREATE INDEX idx_discovery_events_vm_id_discovered_at ON discovery_events(vm_id, discovered_at);


