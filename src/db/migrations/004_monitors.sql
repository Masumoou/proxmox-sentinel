CREATE TABLE monitors (
    id TEXT PRIMARY KEY,
    resource_id TEXT NOT NULL REFERENCES resources(id),
    state TEXT NOT NULL,
    interval_secs INTEGER NOT NULL,
    collection_type TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME
);
CREATE INDEX idx_monitors_resource_id_state ON monitors(resource_id, state);


