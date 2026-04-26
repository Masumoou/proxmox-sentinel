use anyhow::{Context, Result};
use std::io::{self, Write};
use std::path::Path;

pub fn run_init(config_path: &Path, force: bool) -> Result<()> {
    let api_url = prompt("Proxmox API URL", "https://127.0.0.1:8006")?;
    let token_id = prompt("API token ID", "sentinel@pve!monitoring")?;
    let token_secret = prompt_secret("API token secret")?;
    let listen_port = prompt("Listen port", "9101")?.parse::<u16>().context("listen port must be a number")?;
    let verify_tls = prompt_bool("Verify TLS certificates?", false)?;
    let dashboard_auth = prompt_bool("Enable dashboard auth?", false)?;
    let prometheus = prompt_bool("Enable Prometheus endpoint?", true)?;

    if config_path.exists() && !force {
        anyhow::bail!("{} already exists. Re-run with --force to overwrite.", config_path.display());
    }

    let auth_line = if dashboard_auth {
        let user = prompt("Dashboard username", "admin")?;
        let pass = prompt_secret("Dashboard password")?;
        format!("auth = \"{}:{}\"", user, pass)
    } else {
        "auth = \"\"".to_string()
    };

    let cfg = format!(
        r#"[proxmox]
api_url = "{api_url}"
api_token_id = "{token_id}"
api_token_secret = "{token_secret}"
nodes = []
insecure_tls = {insecure_tls}

[metrics]
listen_addr = "0.0.0.0"
listen_port = {listen_port}
{auth_line}
prometheus_enabled = {prometheus}

[logs]
tail_lines = 100
buffer_size = 10000
watch_paths = []

[alerts]
enabled = true
webhook_url = ""
cpu_threshold = 90.0
memory_threshold = 85.0
disk_threshold = 90.0

[ssh]
private_key_path = "/root/.ssh/id_ed25519"
user = "root"
timeout_secs = 10
skip_vmids = []

[collection]
api_interval_secs = 15
cgroup_interval_secs = 5
vm_interval_secs = 30
service_check_interval_secs = 60

[services]
auto_discover = true
alert_on_discovered = true

[haproxy]
enabled = false
stats_url = "http://127.0.0.1:8404/stats;csv"
interval_secs = 10

[storage]
db_path = "/var/lib/proxmox-sentinel/sentinel.db"
metric_retention_days = 7
log_retention_days = 14
alert_retention_days = 30

[cluster]
mode = "standalone"
server_url = "http://127.0.0.1:{listen_port}"
shared_secret = "change_me"

[platform]
enabled = true
interval_secs = 60
backup_warn_hours = 48
backup_critical_hours = 72
task_long_running_minutes = 60
snapshot_warn_days = 7
snapshot_max_count = 5
zfs_usage_threshold = 80.0
lvmthin_data_warn_pct = 85.0
lvmthin_data_critical_pct = 95.0
lvmthin_metadata_warn_pct = 75.0
lvmthin_metadata_critical_pct = 90.0
security_enabled = true
exclude_backup_vmids = []
exclude_guest_agent_vmids = []
exclude_snapshot_vmids = []
ignore_templates = true
ignore_stopped_guests_for_backup = true

[backup_policy]
enabled = true
default_required = true
ignore_stopped_guests = true
ignore_templates = true
warn_hours = 48
critical_hours = 72
exclude_vmids = []
include_tags = []
exclude_tags = ["nobackup", "test", "template"]

[[backup_policy.tag_rules]]
tag = "critical"
warn_hours = 24
critical_hours = 36
required = true

[[backup_policy.tag_rules]]
tag = "daily-backup"
warn_hours = 36
critical_hours = 48
required = true

[[backup_policy.tag_rules]]
tag = "nobackup"
warn_hours = 48
critical_hours = 72
required = false

[certificates]
warn_days = 30
critical_days = 7
"#,
        insecure_tls = !verify_tls,
    );

    if let Some(parent) = config_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| format!("creating {}", parent.display()))?;
    }
    std::fs::write(config_path, cfg).with_context(|| format!("writing {}", config_path.display()))?;

    let service_path = Path::new("/etc/systemd/system/proxmox-sentinel.service");
    if service_path.exists() && !force {
        println!("systemd service already exists: {}", service_path.display());
    } else {
        let service = format!(
            r#"[Unit]
Description=Proxmox Sentinel
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=/usr/local/bin/proxmox-sentinel --config {}
Restart=always
RestartSec=5
User=root
Environment=RUST_LOG=info

[Install]
WantedBy=multi-user.target
"#,
            config_path.display()
        );
        std::fs::write(service_path, service).with_context(|| format!("writing {}", service_path.display()))?;
    }

    println!("Wrote {}", config_path.display());
    println!("Wrote {}", service_path.display());
    println!("Next: systemctl daemon-reload && systemctl enable --now proxmox-sentinel");
    Ok(())
}
