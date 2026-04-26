use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(
    name = "proxmox-sentinel",
    version,
    about = "Agentless Proxmox monitoring daemon"
)]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, default_value = "/etc/proxmox-sentinel/config.toml")]
    pub config: PathBuf,

    #[command(subcommand)]
    pub cmd: Option<Commands>,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Interactive first-run setup. Writes config.toml and systemd service.
    Init {
        /// Overwrite existing config/service files
        #[arg(long)]
        force: bool,
    },
    /// Check API, host permissions, port binding, config, and systemd install.
    Doctor,
    /// Print example configuration to stdout
    PrintConfig,
    /// Test Proxmox API connectivity and print node list
    TestApi,
}

impl Cli {
    pub fn parse_args() -> Self {
        <Self as clap::Parser>::parse()
    }
}
