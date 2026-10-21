//! nvml-exporter command line entry point: `serve`, `once` and `check`.

use std::io::Write;
use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};
use nvml_exporter::collector::{Collector, NvmlCollector};
use nvml_exporter::config::{self, Config};
use nvml_exporter::expo::render;

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
    /// Run the daemon (default)
    Serve,
    /// Collect one scrape and print it to stdout
    Once,
    /// Parse the config and print the resolved values
    Check,
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    env_logger::Builder::from_env(env_logger::Env::new().filter_or("NVML_EXPORTER_LOG", "info"))
        .format_timestamp(None)
        .target(env_logger::Target::Stderr)
        .init();

    let cfg = match config::load(&cli.config) {
        Ok(cfg) => cfg,
        Err(e) => {
            eprintln!("nvml-exporter: {e}");
            return ExitCode::from(1);
        }
    };

    match cli.cmd.unwrap_or(Cmd::Serve) {
        Cmd::Serve => {
            eprintln!("serve: not implemented until the HTTP server lands");
            ExitCode::from(2)
        }
        Cmd::Once => once(&cfg),
        Cmd::Check => check(&cfg),
    }
}

fn once(cfg: &Config) -> ExitCode {
    let scrape = NvmlCollector::new(cfg.collect_pcie).collect();
    let mut out = std::io::stdout().lock();
    if out
        .write_all(render(&scrape.samples).as_bytes())
        .and_then(|()| out.flush())
        .is_err()
    {
        return ExitCode::from(1);
    }
    ExitCode::from(u8::from(!scrape.up))
}

fn check(cfg: &Config) -> ExitCode {
    let readers: Vec<String> = cfg
        .readers
        .entries()
        .iter()
        .map(ToString::to_string)
        .collect();
    println!("listen = {}", cfg.listen);
    println!("readers = [{}]", readers.join(", "));
    println!("collect_pcie = {}", cfg.collect_pcie);
    ExitCode::SUCCESS
}
