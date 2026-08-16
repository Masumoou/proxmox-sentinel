CREATE TABLE rules (
    id TEXT PRIMARY KEY,
    metric_id TEXT NOT NULL REFERENCES metrics(id),
    template_id TEXT REFERENCES templates(id),
    state TEXT NOT NULL,
    severity TEXT NOT NULL,
    fire_operator TEXT NOT NULL,
    fire_value_type TEXT NOT NULL CHECK (fire_value_type IN ('number', 'string', 'state')),
    fire_value TEXT NOT NULL,
    fire_duration_secs INTEGER NOT NULL CHECK (fire_duration_secs > 0),
    resolve_operator TEXT NOT NULL,
    resolve_value_type TEXT NOT NULL CHECK (resolve_value_type IN ('number', 'string', 'state')),
    resolve_value TEXT NOT NULL,
    resolve_duration_secs INTEGER NOT NULL CHECK (resolve_duration_secs > 0),
    version INTEGER NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME
);
CREATE INDEX idx_rules_metric_id_state ON rules(metric_id, state);


