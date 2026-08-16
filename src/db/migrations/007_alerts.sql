CREATE TABLE alerts (
    id TEXT PRIMARY KEY,
    rule_id TEXT NOT NULL REFERENCES rules(id),
    state TEXT NOT NULL,
    fired_at DATETIME,
    resolved_at DATETIME
);
CREATE INDEX idx_alerts_rule_id_state ON alerts(rule_id, state);


