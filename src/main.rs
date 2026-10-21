//! nvml-exporter command line entry point.

use std::path::PathBuf;

use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "nvml-exporter", version)]
struct Cli {
    #[arg(long, global = true, default_value = "/etc/nvml-exporter/config.toml")]
    config: PathBuf,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    Serve,
    Once,
    Check,
}

fn main() {
    let cli = Cli::parse();
    let name = match cli.cmd.unwrap_or(Cmd::Serve) {
        Cmd::Serve => "serve",
        Cmd::Once => "once",
        Cmd::Check => "check",
    };
    println!("{name}");
}
