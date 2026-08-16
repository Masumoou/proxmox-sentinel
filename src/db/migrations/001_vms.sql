CREATE TABLE vms (
    id TEXT PRIMARY KEY,
    proxmox_vmid INTEGER NOT NULL,
    node_name TEXT NOT NULL,
    name TEXT NOT NULL,
    os_type TEXT,
    created_at DATETIME NOT NULL,
    updated_at DATETIME NOT NULL,
    deleted_at DATETIME
);


