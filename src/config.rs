use std::path::PathBuf;

use clap::{command, Parser, Subcommand};
use owo_colors::OwoColorize;

/// Main entrypoing to `koba`.
pub fn run() -> eyre::Result<()> {
    let config = Config::parse();
    config.command.run()
}

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Config {
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    #[command(name = "generate")]
    Generate(Generate),
    #[command(name = "deploy")]
    Deploy(Deploy),
}

impl Commands {
    pub fn run(&self) -> eyre::Result<()> {
        match self {
            Commands::Generate(command) => command.run(),
            Commands::Deploy(command) => command.run(),
        }
    }
}

/// Generate deployment transaction data for Stylus contracts.
#[derive(Parser, Debug)]
pub struct Generate {
    /// Path to the contract's compiled webassembly.
    #[arg(long)]
    pub wasm: PathBuf,
    /// Path to the contract's Solidity constructor code.
    #[arg(long)]
    pub sol: PathBuf,
    /// ABI-encoded constructor arguments.
    #[arg(long)]
    pub args: Option<String>,
}

const STYLUS_TESTNET_RPC: &str = "https://stylusv2.arbitrum.io/rpc";

/// Deploy & activate a Stylus contract.
#[derive(Parser, Debug)]
pub struct Deploy {
    #[command(flatten)]
    pub generate_config: Generate,
    #[command(flatten)]
    pub auth: PrivateKey,
    /// Arbitrum RPC endpoint.
    #[arg(short = 'e', long, default_value = STYLUS_TESTNET_RPC)]
    pub endpoint: String,
}

impl Deploy {
    pub fn run(&self) -> eyre::Result<()> {
        let _result = crate::deploy(self)?;
        println!("{}", "Success!".bright_green());
        Ok(())
    }
}

#[derive(Parser, Debug)]
#[group(required = true)]
pub struct PrivateKey {
    /// File path to a text file containing a hex-encoded private key.
    #[arg(long)]
    pub private_key_path: Option<PathBuf>,
    /// Private key as a hex string. Warning: this exposes your key to shell
    /// history.
    #[arg(long)]
    pub private_key: Option<String>,
    /// Path to an Ethereum wallet keystore file (e.g. clef).
    #[arg(long)]
    pub keystore_path: Option<String>,
    /// Keystore password file.
    #[arg(long)]
    pub keystore_password_path: Option<PathBuf>,
}
