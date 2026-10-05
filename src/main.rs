mod cli;
mod command;
mod config;
mod daemon;
mod error;
mod grid;
mod ipc;
mod mode;
mod naming;
mod navigation;
mod outputs;
mod plan;
mod reconcile;
mod world;

use std::process::ExitCode;

use clap::Parser;

use cli::{Cli, Cmd};
use config::Config;
use ipc::Connection;

fn run(config: &Config, command: Cmd) -> error::Result<()> {
    match command {
        Cmd::Daemon => daemon::run(config),
        command => plan::execute(&mut Connection::connect()?, config, command),
    }
}

fn main() -> ExitCode {
    let (config, command) = Cli::parse().into_parts().unwrap_or_else(|e| e.exit());
    match run(&config, command) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("i3-spatial-desktop: {e}");
            ExitCode::FAILURE
        }
    }
}
