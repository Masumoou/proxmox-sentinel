CREATE TABLE notification_routes (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    rule_id TEXT,
    severity TEXT,
    scope_type TEXT,
    scope_id TEXT,
    priority INTEGER NOT NULL DEFAULT 0,
    template_id TEXT NOT NULL,
    channel_id TEXT NOT NULL,
    state TEXT NOT NULL,
    version INTEGER NOT NULL DEFAULT 1,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME,
    FOREIGN KEY(rule_id) REFERENCES rules(id),
    FOREIGN KEY(template_id) REFERENCES templates(id),
    FOREIGN KEY(channel_id) REFERENCES notification_channels(id)
);
