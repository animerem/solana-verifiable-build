use anyhow::{anyhow, Result};
use cargo_lock::Lockfile;
use cargo_toml::Manifest;
use clap::{Parser, Subcommand};
use signal_hook::{
    consts::{SIGINT, SIGTERM},
    iterator::Signals,
};
use solana_cli_config::{Config, CONFIG_FILE};
use solana_client::rpc_client::RpcClient;
use solana_sdk::{
    bpf_loader_upgradeable::{self, UpgradeableLoaderState},
    pubkey::Pubkey,
};
use std::{
    fs::File,
    io::Read,
    path::PathBuf,
    process::{exit, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};
use uuid::Uuid;

mod api;
mod image_config;
mod solana_program;

use image_config::IMAGE_MAP;
use crate::{
    api::send_job_to_remote,
    solana_program::{process_close, upload_program},
};

const MAINNET_GENESIS_HASH: &str = "5eykt4UsFv8P8NJdTREpY1vzqKqZKvdpKuc147dw2N9d";

fn get_network_url(network_str: &str) -> &str {
    match network_str {
        "devnet" | "dev" | "d" => "https://api.devnet.solana.com",
        "mainnet" | "main" | "m" => "https://api.mainnet-beta.solana.com",
        _ => "https://api.devnet.solana.com",
    }
}

#[derive(Parser, Debug)]
#[command(name = "solana-verifiable-build")]
#[command(about = "Tool for verifying and uploading Solana programs", long_about = None)]
struct Cli {
    #[arg(short, long, default_value = "devnet")]
    network: String,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand, Debug)]
enum Commands {
    Upload {
        #[arg(short, long)]
        program_path: PathBuf,
    },
    Close {
        #[arg(short, long)]
        program_id: String,
    },
}

fn setup_signal_handler(terminated: Arc<AtomicBool>) {
    let mut signals = Signals::new(&[SIGINT, SIGTERM]).expect("Unable to setup signal handling");
    std::thread::spawn(move || {
        for _sig in signals.forever() {
            terminated.store(true, Ordering::Relaxed);
            break;
        }
    });
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let rpc_url = get_network_url(&cli.network);
    let rpc_client = RpcClient::new(rpc_url.to_string());

    let config = Config::load(CONFIG_FILE)?;
    let payer = solana_sdk::signature::read_keypair_file(&config.keypair_path)
        .map_err(|_| anyhow!("Failed to read keypair file"))?;

    let terminated = Arc::new(AtomicBool::new(false));
    setup_signal_handler(terminated.clone());

    match &cli.command {
        Commands::Upload { program_path } => {
            upload_program(&rpc_client, &payer, program_path, &terminated)?;
        }
        Commands::Close { program_id } => {
            let pubkey: Pubkey = program_id.parse()?;
            process_close(&rpc_client, &payer, &pubkey, &terminated)?;
        }
    }
    Ok(())
}
