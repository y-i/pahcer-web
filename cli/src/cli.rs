use std::{path::PathBuf, process::ExitCode};

use clap::{Args, Parser, Subcommand};
use futures_util::StreamExt;
use tokio::io::{self, AsyncWriteExt};

use crate::{
    error::AppError,
    models::StreamMessage,
    server::{UiServerOptions, start_ui_server},
};

#[derive(Debug, Parser)]
#[command(name = "pahcer-web", about = "Web UI for pahcer", version)]
pub struct Cli {
    #[arg(short = 'C', long, default_value = ".")]
    pub directory: PathBuf,
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    Ui(UiArgs),
    Run(RunArgs),
}

#[derive(Debug, Args)]
pub struct UiArgs {
    #[arg(short, long, default_value_t = 10432)]
    pub port: u16,
    #[arg(long = "no-build", default_value_t = false)]
    pub no_build: bool,
}

#[derive(Debug, Args)]
pub struct RunArgs {
    #[arg(short, long, default_value_t = 10432)]
    pub port: u16,
    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    pub args: Vec<String>,
}

pub async fn run_cli(cli: Cli) -> Result<ExitCode, AppError> {
    match cli.command {
        Commands::Ui(args) => {
            start_ui_server(UiServerOptions {
                base_dir: cli.directory,
                port: args.port,
                build_frontend: !args.no_build,
                frontend_dir: None,
                pahcer_program: None,
            })
            .await?;
            Ok(ExitCode::SUCCESS)
        }
        Commands::Run(args) => bridge_run_command(args.port, args.args).await,
    }
}

async fn bridge_run_command(port: u16, args: Vec<String>) -> Result<ExitCode, AppError> {
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://localhost:{port}/api/run"))
        .json(&serde_json::json!({ "args": args }))
        .send()
        .await
        .map_err(|error| {
            AppError::CommandFailed(format!(
                "Failed to connect to pahcer-web server on port {port}: {error}"
            ))
        })?;

    if !response.status().is_success() {
        let status = response.status();
        let text = response.text().await.unwrap_or_default();
        return Err(AppError::CommandFailed(format!(
            "Server returned error: {status} {text}"
        )));
    }

    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    let mut buffer = String::new();
    let mut exit_code = 0;
    let mut stream = response.bytes_stream();

    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        buffer.push_str(&String::from_utf8_lossy(&chunk));
        while let Some(position) = buffer.find('\n') {
            let line = buffer[..position].to_string();
            buffer.drain(..=position);
            if line.trim().is_empty() {
                continue;
            }

            if let Ok(message) = serde_json::from_str::<serde_json::Value>(&line) {
                match serde_json::from_value::<StreamMessage>(message)? {
                    StreamMessage::Stdout { data } => stdout.write_all(data.as_bytes()).await?,
                    StreamMessage::Stderr { data } => stderr.write_all(data.as_bytes()).await?,
                    StreamMessage::Exit { code } => exit_code = code,
                }
            }
        }
    }

    stdout.flush().await?;
    stderr.flush().await?;
    Ok(ExitCode::from(exit_code as u8))
}

#[cfg(test)]
mod tests {
    use super::Cli;
    use clap::{CommandFactory, Parser};

    #[test]
    fn clap_definition_is_valid() {
        Cli::command().debug_assert();
    }

    #[test]
    fn trailing_args_accept_unknown_options() {
        let cli =
            Cli::try_parse_from(["pahcer-web", "run", "-c", "memo", "--tag", "nightly"]).unwrap();
        match cli.command {
            super::Commands::Run(args) => {
                assert_eq!(args.args, vec!["-c", "memo", "--tag", "nightly"]);
            }
            _ => panic!("expected run command"),
        }
    }
}
