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

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    // --- Global flag parsing ---

    #[test]
    fn parse_device_list_defaults() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "device", "list"])?;
        assert!(!cli.json);
        assert_eq!(cli.verbose, 0);
        assert!(cli.endpoint.is_none());
        assert!(matches!(
            cli.command,
            Commands::Device(DeviceCommands::List { detailed: false })
        ));
        Ok(())
    }

    #[test]
    fn parse_global_json_flag_before_subcommand() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "--json", "device", "list"])?;
        assert!(cli.json);
        Ok(())
    }

    #[test]
    fn parse_global_json_flag_after_subcommand() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "device", "list", "--json"])?;
        assert!(cli.json);
        Ok(())
    }

    #[test]
    fn parse_verbose_levels() -> TestResult {
        let cli0 = Cli::try_parse_from(["wheelctl", "device", "list"])?;
        assert_eq!(cli0.verbose, 0);

        let cli1 = Cli::try_parse_from(["wheelctl", "-v", "device", "list"])?;
        assert_eq!(cli1.verbose, 1);

        let cli2 = Cli::try_parse_from(["wheelctl", "-vv", "device", "list"])?;
        assert_eq!(cli2.verbose, 2);

        let cli3 = Cli::try_parse_from(["wheelctl", "-vvv", "device", "list"])?;
        assert_eq!(cli3.verbose, 3);
        Ok(())
    }

    #[test]
    fn parse_endpoint_flag() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "--endpoint",
            "http://localhost:5000",
            "device",
            "list",
        ])?;
        assert_eq!(cli.endpoint.as_deref(), Some("http://localhost:5000"));
        Ok(())
    }

    // --- Device command parsing ---

    #[test]
    fn parse_device_list_detailed() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "device", "list", "--detailed"])?;
        assert!(matches!(
            cli.command,
            Commands::Device(DeviceCommands::List { detailed: true })
        ));
        Ok(())
    }

    #[test]
    fn parse_device_status() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "device", "status", "wheel-001"])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Status { device, watch }) => {
                assert_eq!(device, "wheel-001");
                assert!(!watch);
            }
            _ => return Err("expected Device Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_status_watch() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "device", "status", "wheel-001", "--watch"])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Status { watch, .. }) => {
                assert!(watch);
            }
            _ => return Err("expected Device Status command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_calibrate() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "device",
            "calibrate",
            "wheel-001",
            "center",
            "--yes",
        ])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Calibrate {
                device,
                calibration_type,
                yes,
            }) => {
                assert_eq!(device, "wheel-001");
                assert!(matches!(calibration_type, CalibrationType::Center));
                assert!(yes);
            }
            _ => return Err("expected Device Calibrate command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_device_calibrate_all_types() -> TestResult {
        for (arg, expected) in [
            ("center", CalibrationType::Center),
            ("dor", CalibrationType::Dor),
            ("pedals", CalibrationType::Pedals),
            ("all", CalibrationType::All),
        ] {
            let cli = Cli::try_parse_from(["wheelctl", "device", "calibrate", "w1", arg])?;
            match &cli.command {
                Commands::Device(DeviceCommands::Calibrate {
                    calibration_type, ..
                }) => {
                    assert_eq!(
                        std::mem::discriminant(calibration_type),
                        std::mem::discriminant(&expected)
                    );
                }
                _ => return Err("expected Device Calibrate command".into()),
            }
        }
        Ok(())
    }

    #[test]
    fn parse_device_reset_force() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "device", "reset", "dev-001", "--force"])?;
        match &cli.command {
            Commands::Device(DeviceCommands::Reset { device, force }) => {
                assert_eq!(device, "dev-001");
                assert!(force);
            }
            _ => return Err("expected Device Reset command".into()),
        }
        Ok(())
    }

    // --- Profile command parsing ---

    #[test]
    fn parse_profile_list_with_filters() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl", "profile", "list", "--game", "iracing", "--car", "gt3",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::List { game, car }) => {
                assert_eq!(game.as_deref(), Some("iracing"));
                assert_eq!(car.as_deref(), Some("gt3"));
            }
            _ => return Err("expected Profile List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_list_no_filters() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "profile", "list"])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::List { game, car }) => {
                assert!(game.is_none());
                assert!(car.is_none());
            }
            _ => return Err("expected Profile List command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_apply_with_skip_validation() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "profile",
            "apply",
            "dev-001",
            "my_profile.json",
            "--skip-validation",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Apply {
                device,
                profile,
                skip_validation,
            }) => {
                assert_eq!(device, "dev-001");
                assert_eq!(profile, "my_profile.json");
                assert!(skip_validation);
            }
            _ => return Err("expected Profile Apply command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_create_with_options() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "profile",
            "create",
            "out.json",
            "--from",
            "base.json",
            "--game",
            "acc",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Create {
                path,
                from,
                game,
                car,
            }) => {
                assert_eq!(path, "out.json");
                assert_eq!(from.as_deref(), Some("base.json"));
                assert_eq!(game.as_deref(), Some("acc"));
                assert!(car.is_none());
            }
            _ => return Err("expected Profile Create command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_edit_with_field_value() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "profile",
            "edit",
            "p.json",
            "--field",
            "base.ffbGain",
            "--value",
            "0.9",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Edit {
                profile,
                field,
                value,
            }) => {
                assert_eq!(profile, "p.json");
                assert_eq!(field.as_deref(), Some("base.ffbGain"));
                assert_eq!(value.as_deref(), Some("0.9"));
            }
            _ => return Err("expected Profile Edit command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_validate() -> TestResult {
        let cli =
            Cli::try_parse_from(["wheelctl", "profile", "validate", "test.json", "--detailed"])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Validate { path, detailed }) => {
                assert_eq!(path, "test.json");
                assert!(detailed);
            }
            _ => return Err("expected Profile Validate command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_export_signed() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl", "profile", "export", "p.json", "--output", "out.json", "--signed",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Export {
                profile,
                output,
                signed,
            }) => {
                assert_eq!(profile, "p.json");
                assert_eq!(output.as_deref(), Some("out.json"));
                assert!(signed);
            }
            _ => return Err("expected Profile Export command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_profile_import_with_verify() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "profile",
            "import",
            "in.json",
            "--target",
            "dest.json",
            "--verify",
        ])?;
        match &cli.command {
            Commands::Profile(ProfileCommands::Import {
                path,
                target,
                verify,
            }) => {
                assert_eq!(path, "in.json");
                assert_eq!(target.as_deref(), Some("dest.json"));
                assert!(verify);
            }
            _ => return Err("expected Profile Import command".into()),
        }
        Ok(())
    }

    // --- Plugin command parsing ---

    #[test]
    fn parse_plugin_install_with_version() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "plugin",
            "install",
            "ffb-smoothing",
            "--version",
            "1.2.0",
        ])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Install { plugin_id, version }) => {
                assert_eq!(plugin_id, "ffb-smoothing");
                assert_eq!(version.as_deref(), Some("1.2.0"));
            }
            _ => return Err("expected Plugin Install command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_search() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "plugin", "search", "smoothing"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Search { query }) => {
                assert_eq!(query, "smoothing");
            }
            _ => return Err("expected Plugin Search command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_uninstall_force() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "plugin", "uninstall", "my-plugin", "--force"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Uninstall { plugin_id, force }) => {
                assert_eq!(plugin_id, "my-plugin");
                assert!(force);
            }
            _ => return Err("expected Plugin Uninstall command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_plugin_verify() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "plugin", "verify", "ffb-smoothing"])?;
        match &cli.command {
            Commands::Plugin(PluginCommands::Verify { plugin_id }) => {
                assert_eq!(plugin_id, "ffb-smoothing");
            }
            _ => return Err("expected Plugin Verify command".into()),
        }
        Ok(())
    }

    // --- Safety command parsing ---

    #[test]
    fn parse_safety_limit_global() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "safety",
            "limit",
            "wheel-001",
            "5.5",
            "--global",
        ])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Limit {
                device,
                torque,
                global,
            }) => {
                assert_eq!(device, "wheel-001");
                assert!((torque - 5.5).abs() < f32::EPSILON);
                assert!(global);
            }
            _ => return Err("expected Safety Limit command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_safety_stop_all() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "safety", "stop"])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Stop { device }) => {
                assert!(device.is_none());
            }
            _ => return Err("expected Safety Stop command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_safety_stop_specific_device() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "safety", "stop", "wheel-001"])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Stop { device }) => {
                assert_eq!(device.as_deref(), Some("wheel-001"));
            }
            _ => return Err("expected Safety Stop command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_safety_enable_force() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "safety", "enable", "wheel-001", "--force"])?;
        match &cli.command {
            Commands::Safety(SafetyCommands::Enable { device, force }) => {
                assert_eq!(device, "wheel-001");
                assert!(force);
            }
            _ => return Err("expected Safety Enable command".into()),
        }
        Ok(())
    }

    // --- Diag command parsing ---

    #[test]
    fn parse_diag_record_with_defaults() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "diag", "record", "wheel-001"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Record {
                device, duration, ..
            }) => {
                assert_eq!(device, "wheel-001");
                assert_eq!(*duration, 120);
            }
            _ => return Err("expected Diag Record command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_record_custom_duration() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "diag",
            "record",
            "wheel-001",
            "--duration",
            "60",
            "--output",
            "test.wbb",
        ])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Record {
                device,
                duration,
                output,
            }) => {
                assert_eq!(device, "wheel-001");
                assert_eq!(*duration, 60);
                assert_eq!(output.as_deref(), Some("test.wbb"));
            }
            _ => return Err("expected Diag Record command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_test_specific_type() -> TestResult {
        let cli =
            Cli::try_parse_from(["wheelctl", "diag", "test", "--device", "wheel-001", "motor"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Test { device, test_type }) => {
                assert_eq!(device.as_deref(), Some("wheel-001"));
                assert!(matches!(test_type, Some(TestType::Motor)));
            }
            _ => return Err("expected Diag Test command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_diag_metrics_watch() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "diag", "metrics", "--watch"])?;
        match &cli.command {
            Commands::Diag(DiagCommands::Metrics { watch, .. }) => {
                assert!(watch);
            }
            _ => return Err("expected Diag Metrics command".into()),
        }
        Ok(())
    }

    // --- Telemetry command parsing ---

    #[test]
    fn parse_telemetry_probe() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "telemetry",
            "probe",
            "--game",
            "acc",
            "--endpoint",
            "127.0.0.1:9001",
            "--timeout-ms",
            "200",
            "--attempts",
            "5",
        ])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Probe {
                game,
                endpoint,
                timeout_ms,
                attempts,
            }) => {
                assert_eq!(game, "acc");
                assert_eq!(endpoint, "127.0.0.1:9001");
                assert_eq!(*timeout_ms, 200);
                assert_eq!(*attempts, 5);
            }
            _ => return Err("expected Telemetry Probe command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_probe_defaults() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "telemetry", "probe", "--game", "acc"])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Probe {
                endpoint,
                timeout_ms,
                attempts,
                ..
            }) => {
                assert_eq!(endpoint, "127.0.0.1:9000");
                assert_eq!(*timeout_ms, 400);
                assert_eq!(*attempts, 3);
            }
            _ => return Err("expected Telemetry Probe command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_telemetry_capture() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "telemetry",
            "capture",
            "--game",
            "acc",
            "--port",
            "9001",
            "--duration",
            "30",
            "--out",
            "capture.bin",
            "--max-payload",
            "1024",
        ])?;
        match &cli.command {
            Commands::Telemetry(TelemetryCommands::Capture {
                game,
                port,
                duration,
                out,
                max_payload,
            }) => {
                assert_eq!(game, "acc");
                assert_eq!(*port, 9001);
                assert_eq!(*duration, 30);
                assert_eq!(out, "capture.bin");
                assert_eq!(*max_payload, 1024);
            }
            _ => return Err("expected Telemetry Capture command".into()),
        }
        Ok(())
    }

    // --- Game command parsing ---

    #[test]
    fn parse_game_configure() -> TestResult {
        let cli = Cli::try_parse_from([
            "wheelctl",
            "game",
            "configure",
            "iracing",
            "--path",
            "/games/iracing",
            "--auto",
        ])?;
        match &cli.command {
            Commands::Game(GameCommands::Configure { game, path, auto }) => {
                assert_eq!(game, "iracing");
                assert_eq!(path.as_deref(), Some("/games/iracing"));
                assert!(auto);
            }
            _ => return Err("expected Game Configure command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_test_custom_duration() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "game", "test", "acc", "--duration", "30"])?;
        match &cli.command {
            Commands::Game(GameCommands::Test { game, duration }) => {
                assert_eq!(game, "acc");
                assert_eq!(*duration, 30);
            }
            _ => return Err("expected Game Test command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_game_test_default_duration() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "game", "test", "acc"])?;
        match &cli.command {
            Commands::Game(GameCommands::Test { duration, .. }) => {
                assert_eq!(*duration, 10);
            }
            _ => return Err("expected Game Test command".into()),
        }
        Ok(())
    }

    // --- Completion and health ---

    #[test]
    fn parse_completion_bash() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "completion", "bash"])?;
        assert!(matches!(cli.command, Commands::Completion { .. }));
        Ok(())
    }

    #[test]
    fn parse_health_no_watch() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "health"])?;
        match &cli.command {
            Commands::Health { watch } => assert!(!watch),
            _ => return Err("expected Health command".into()),
        }
        Ok(())
    }

    #[test]
    fn parse_health_watch() -> TestResult {
        let cli = Cli::try_parse_from(["wheelctl", "health", "--watch"])?;
        match &cli.command {
            Commands::Health { watch } => assert!(watch),
            _ => return Err("expected Health command".into()),
        }
        Ok(())
    }

    // --- Rejection / error cases ---

    #[test]
    fn reject_no_subcommand() {
        let result = Cli::try_parse_from(["wheelctl"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_unknown_subcommand() {
        let result = Cli::try_parse_from(["wheelctl", "nonexistent"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_missing_required_device_arg() {
        let result = Cli::try_parse_from(["wheelctl", "device", "status"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_invalid_calibration_type() {
        let result = Cli::try_parse_from(["wheelctl", "device", "calibrate", "w1", "invalid_type"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_invalid_test_type() {
        let result = Cli::try_parse_from(["wheelctl", "diag", "test", "bad_type"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_missing_plugin_search_query() {
        let result = Cli::try_parse_from(["wheelctl", "plugin", "search"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_missing_completion_shell() {
        let result = Cli::try_parse_from(["wheelctl", "completion"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_safety_limit_missing_torque() {
        let result = Cli::try_parse_from(["wheelctl", "safety", "limit", "wheel-001"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_safety_limit_non_numeric_torque() {
        let result = Cli::try_parse_from(["wheelctl", "safety", "limit", "wheel-001", "abc"]);
        assert!(result.is_err());
    }

    #[test]
    fn reject_unknown_device_subcommand() {
        let result = Cli::try_parse_from(["wheelctl", "device", "fly"]);
        assert!(result.is_err());
    }
}
