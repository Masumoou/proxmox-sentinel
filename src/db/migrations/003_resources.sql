CREATE TABLE resources (
    id TEXT PRIMARY KEY,
    vm_id TEXT NOT NULL REFERENCES vms(id),
    kind TEXT NOT NULL,
    identifier TEXT NOT NULL,
    state TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,
    UNIQUE(vm_id, kind, identifier)
);
CREATE INDEX idx_resources_vm_id_state ON resources(vm_id, state);


