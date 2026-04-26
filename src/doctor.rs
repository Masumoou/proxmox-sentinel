use anyhow::{Context, Result};
use std::net::TcpListener;
use std::path::Path;
use std::time::Duration;

use crate::config::Config;
use crate::proxmox_api::ProxmoxClient;

pub async fn run_doctor(config_path: &Path) -> Result<()> {
    let mut failures = 0usize;
    let mut check = |name: &str, result: Result<String>| match result {
        Ok(detail) => println!("OK   {name}: {detail}"),
        Err(e) => {
            failures += 1;
            println!("FAIL {name}: {e}");
        }
    };

    check(
        "config file",
        Config::load(config_path).and_then(|cfg| {
            cfg.validate()?;
            Ok(format!("valid ({})", config_path.display()))
        }),
    );

    let cfg = Config::load(config_path)?;
    let client = ProxmoxClient::new(&cfg.proxmox)?;

    check(
        "Proxmox API",
        async {
            client
                .list_nodes()
                .await
                .map(|nodes| format!("connected, {} nodes", nodes.len()))
        }
        .await,
    );

    let nodes = client.list_nodes().await.unwrap_or_default();
    check(
        "list nodes",
        if nodes.is_empty() {
            anyhow::bail!("no nodes returned")
        } else {
            Ok(nodes.join(", "))
        },
    );

    let mut guest_count = 0usize;
    for node in &nodes {
        guest_count += client.list_guests(node).await.unwrap_or_default().len();
    }
    check(
        "list guests",
        if guest_count == 0 {
            anyhow::bail!("no guests returned")
        } else {
            Ok(format!("{guest_count} guests"))
        },
    );

    check(
        "cgroup access",
        std::fs::read_dir("/sys/fs/cgroup")
            .map(|_| "/sys/fs/cgroup readable".to_string())
            .map_err(Into::into),
    );

    check(
        "LXC rootfs logs",
        std::fs::read_dir("/var/lib/lxc")
            .map(|_| "/var/lib/lxc readable".to_string())
            .map_err(Into::into),
    );

    check("bind port", check_port_or_running_sentinel(&cfg).await);

    check(
        "systemd service",
        if Path::new("/etc/systemd/system/proxmox-sentinel.service").exists() {
            Ok("installed".to_string())
        } else {
            anyhow::bail!("missing /etc/systemd/system/proxmox-sentinel.service")
        },
    );

    if failures > 0 {
        anyhow::bail!("{failures} doctor checks failed");
    }
    Ok(())
}

async fn check_port_or_running_sentinel(cfg: &Config) -> Result<String> {
    match TcpListener::bind((cfg.metrics.listen_addr.as_str(), cfg.metrics.listen_port)) {
        Ok(_) => Ok(format!(
            "{}:{} available",
            cfg.metrics.listen_addr, cfg.metrics.listen_port
        )),
        Err(e) if e.kind() == std::io::ErrorKind::AddrInUse => {
            let health_host =
                if cfg.metrics.listen_addr == "0.0.0.0" || cfg.metrics.listen_addr == "::" {
                    "127.0.0.1"
                } else {
                    cfg.metrics.listen_addr.as_str()
                };
            let url = format!("http://{health_host}:{}/health", cfg.metrics.listen_port);
            let client = reqwest::Client::builder()
                .timeout(Duration::from_secs(3))
                .build()
                .context("building doctor health client")?;
            let mut request = client.get(&url);
            if let Some(auth) = cfg
                .metrics
                .auth
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
            {
                if let Some((user, pass)) = auth.split_once(':') {
                    request = request.basic_auth(user.to_string(), Some(pass.to_string()));
                }
            }
            let response = request
                .send()
                .await
                .with_context(|| format!("checking {url}"))?;
            if response.status().is_success() {
                let body = response.text().await.unwrap_or_default();
                if body.trim() == "OK" {
                    return Ok(format!(
                        "{}:{} already in use by running Sentinel (/health OK)",
                        cfg.metrics.listen_addr, cfg.metrics.listen_port
                    ));
                }
            }
            anyhow::bail!(
                "{}:{} is already in use, but Sentinel /health did not return OK",
                cfg.metrics.listen_addr,
                cfg.metrics.listen_port
            )
        }
        Err(e) => Err(e).with_context(|| {
            format!(
                "binding {}:{}",
                cfg.metrics.listen_addr, cfg.metrics.listen_port
            )
        }),
    }
}
