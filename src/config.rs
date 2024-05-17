use std::path::PathBuf;

use clap::{command, Parser, Subcommand};

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
}

impl Commands {
    pub fn run(&self) -> eyre::Result<()> {
        match self {
            Commands::Generate(command) => command.run(),
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
    /// Constructor arguments.
    #[arg(long)]
    pub args: Vec<String>,
}

impl Generate {
    pub fn run(&self) -> eyre::Result<()> {
        crate::generate(self)
    }
}
