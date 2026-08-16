CREATE TABLE telemetry (
    id TEXT PRIMARY KEY,
    metric_id TEXT NOT NULL REFERENCES metrics(id),
    timestamp DATETIME NOT NULL,
    value REAL,
    string_value TEXT,
    observation TEXT NOT NULL,
    labels TEXT
);
CREATE INDEX idx_telemetry_metric_id_timestamp ON telemetry(metric_id, timestamp);


