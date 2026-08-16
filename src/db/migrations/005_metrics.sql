CREATE TABLE metrics (
    id TEXT PRIMARY KEY,
    monitor_id TEXT NOT NULL REFERENCES monitors(id),
    name TEXT NOT NULL,
    value_type TEXT NOT NULL,
    unit TEXT,
    UNIQUE(monitor_id, name)
);
CREATE INDEX idx_metrics_monitor_id ON metrics(monitor_id);


