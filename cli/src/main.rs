use std::process::ExitCode;

use clap::Parser;
use pahcer_web::cli::{Cli, run_cli};

#[tokio::main]
async fn main() -> ExitCode {
    match run_cli(Cli::parse()).await {
        Ok(code) => code,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::from(1)
        }
    }
}
