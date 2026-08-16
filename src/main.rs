// src/main.rs
// proxmox-sentinel entrypoint. Runtime orchestration lives in runtime.rs.
// These remaining allows are intentionally temporary while the older collector
// modules are being refactored in small, behavior-preserving slices.
#![allow(
    clippy::collapsible_if,
    clippy::field_reassign_with_default,
    clippy::manual_flatten,
    clippy::too_many_arguments,
    clippy::unnecessary_cast,
    clippy::unwrap_or_default
)]

use anyhow::Result;
use tracing::info;
use tracing_subscriber::EnvFilter;

mod db;
mod domain;

mod alert_channels;
mod alert_rules;
mod alerts;
mod cli;
mod cluster;
mod collectors;
mod config;
mod doctor;
mod exporter;
mod init;
mod intelligence;
mod proxmox_api;
mod runtime;
mod storage;

use cli::{Cli, Commands};
use config::Config;
use proxmox_api::ProxmoxClient;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_target(false)
        .compact()
        .init();

    let cli = Cli::parse_args();

    match cli.cmd {
        Some(Commands::Init { force }) => {
            init::run_init(&cli.config, force)?;
            return Ok(());
        }
        Some(Commands::Doctor) => {
            doctor::run_doctor(&cli.config).await?;
            return Ok(());
        }
        Some(Commands::PrintConfig) => {
            Config::write_example();
            return Ok(());
        }
        Some(Commands::TestApi) => {
            let cfg = Config::load(&cli.config)?;
            let client = ProxmoxClient::new(&cfg.proxmox)?;
            let nodes = client.list_nodes().await?;
            println!("Connected! Nodes: {:?}", nodes);
            for node in &nodes {
                let status = client.node_status(node).await?;
                println!(
                    "  {}: CPU={:.1}% MEM={:.1}% PVE={}",
                    node,
                    status.cpu_usage * 100.0,
                    (status.mem_used as f64 / status.mem_total as f64) * 100.0,
                    status.pve_version
                );
            }
            return Ok(());
        }
        None => {}
    }

    let cfg = Config::load(&cli.config)?;
    info!("proxmox-sentinel starting");
    runtime::run(cfg).await
}

#[cfg(test)]
mod integration_1_telemetry_gap;
#[cfg(test)]
mod integration_2_maintenance_suppression;
