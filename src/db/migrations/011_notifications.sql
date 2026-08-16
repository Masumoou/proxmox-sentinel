CREATE TABLE notifications (
    id TEXT PRIMARY KEY,
    incident_id TEXT NOT NULL REFERENCES incidents(id),
    route_id TEXT NOT NULL REFERENCES notification_routes(id),
    channel_id TEXT NOT NULL REFERENCES notification_channels(id),
    sent_at DATETIME NOT NULL,
    success BOOLEAN NOT NULL,
    error_message TEXT
);
CREATE INDEX idx_notifications_incident_id ON notifications(incident_id);


