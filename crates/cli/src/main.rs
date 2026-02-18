//! wheelctl - Racing Wheel Control CLI
//!
//! A comprehensive command-line interface for managing racing wheel hardware,
//! profiles, diagnostics, and game integration.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

mod client;
mod commands;
mod completion;
mod error;
mod output;

use anyhow::Result;
use clap::{Parser, Subcommand};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::commands::*;
use crate::error::CliError;

#[derive(Parser)]
#[command(name = "wheelctl")]
#[command(
    about = "Racing Wheel Control CLI - Manage racing wheel hardware, profiles, and diagnostics"
)]
#[command(version)]
#[command(long_about = "
wheelctl is a command-line interface for the Racing Wheel Software Suite.
It provides comprehensive control over racing wheel hardware, profile management,
diagnostics, and game integration features.

All write operations available in the UI are also available through this CLI.
Use --json flag for machine-readable output suitable for scripting.
")]
struct Cli {
    /// Output format (human-readable or JSON)
    #[arg(
        long,
        global = true,
        help = "Output in JSON format for machine parsing"
    )]
    json: bool,

    /// Verbose logging
    #[arg(short, long, global = true, action = clap::ArgAction::Count)]
    verbose: u8,

    /// Service endpoint (for testing)
    #[arg(long, global = true, env = "WHEELCTL_ENDPOINT", hide = true)]
    endpoint: Option<String>,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Device management commands
    #[command(subcommand)]
    Device(DeviceCommands),

    /// Profile management commands  
    #[command(subcommand)]
    Profile(ProfileCommands),

    /// Plugin management commands
    #[command(subcommand)]
    Plugin(PluginCommands),

    /// Diagnostic and monitoring commands
    #[command(subcommand)]
    Diag(DiagCommands),

    /// Game integration commands
    #[command(subcommand)]
    Game(GameCommands),

    /// Telemetry probe and capture commands
    #[command(subcommand)]
    Telemetry(TelemetryCommands),

    /// Safety and control commands
    #[command(subcommand)]
    Safety(SafetyCommands),

    /// Generate shell completion scripts
    Completion {
        /// Shell to generate completion for
        #[arg(value_enum)]
        shell: clap_complete::Shell,
    },

    /// Service health and status
    Health {
        /// Watch health events in real-time
        #[arg(short, long)]
        watch: bool,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize logging based on verbosity
    let log_level = match cli.verbose {
        0 => "warn",
        1 => "info",
        2 => "debug",
        _ => "trace",
    };

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| format!("wheelctl={}", log_level).into()),
        )
        .with(tracing_subscriber::fmt::layer().with_target(false))
        .init();

    // Execute command
    let result = execute_command(&cli).await;

    // Handle errors with appropriate exit codes
    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            if cli.json {
                output::print_error_json(&e);
            } else {
                output::print_error_human(&e);
            }

            // Set appropriate exit code
            let exit_code = match e.downcast_ref::<CliError>() {
                Some(CliError::DeviceNotFound(_)) => 2,
                Some(CliError::ProfileNotFound(_)) => 3,
                Some(CliError::ValidationError(_))
                | Some(CliError::JsonError(_))
                | Some(CliError::SchemaError(_)) => 4,
                Some(CliError::ServiceUnavailable(_)) => 5,
                Some(CliError::PermissionDenied(_)) => 6,
                _ => 1,
            };

            std::process::exit(exit_code);
        }
    }
}

async fn execute_command(cli: &Cli) -> Result<()> {
    match &cli.command {
        Commands::Device(cmd) => {
            commands::device::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Profile(cmd) => {
            commands::profile::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Plugin(cmd) => {
            commands::plugin::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Diag(cmd) => {
            commands::diag::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Game(cmd) => {
            commands::game::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Telemetry(cmd) => commands::telemetry::execute(cmd, cli.json).await,
        Commands::Safety(cmd) => {
            commands::safety::execute(cmd, cli.json, cli.endpoint.as_deref()).await
        }
        Commands::Completion { shell } => {
            completion::generate_completion(*shell);
            Ok(())
        }
        Commands::Health { watch } => {
            commands::health::execute(*watch, cli.json, cli.endpoint.as_deref()).await
        }
    }
}
