CREATE TABLE maintenance_windows (
    id TEXT PRIMARY KEY,
    scope_type TEXT NOT NULL,
    scope_id TEXT,
    start_time DATETIME NOT NULL,
    end_time DATETIME NOT NULL,
    created_by TEXT NOT NULL
);


