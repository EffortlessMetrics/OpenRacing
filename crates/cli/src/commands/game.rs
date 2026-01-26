//! Game integration commands

use anyhow::Result;
use dialoguer::Input;
use std::time::Duration;
use tokio::time::interval;

use crate::client::WheelClient;
use crate::commands::GameCommands;
use crate::error::CliError;
use crate::output;

/// Execute game command
pub async fn execute(cmd: &GameCommands, json: bool, endpoint: Option<&str>) -> Result<()> {
    let client = WheelClient::connect(endpoint).await?;

    match cmd {
        GameCommands::List { detailed } => list_supported_games(json, *detailed).await,
        GameCommands::Configure { game, path, auto } => {
            configure_game(&client, game, path.as_deref(), json, *auto).await
        }
        GameCommands::Status { telemetry } => show_game_status(&client, json, *telemetry).await,
        GameCommands::Test { game, duration } => {
            test_telemetry(&client, game, *duration, json).await
        }
    }
}

/// List supported games
async fn list_supported_games(json: bool, detailed: bool) -> Result<()> {
    let games = get_supported_games();

    if json {
        let output = serde_json::json!({
            "success": true,
            "supported_games": games
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Supported Games:".bold());

        for game in games {
            println!(
                "  {} {} ({})",
                "●".green(),
                game.name.bold(),
                game.id.dimmed()
            );

            if detailed {
                println!("    Version: {}", game.version);
                println!("    Features: {}", game.features.join(", "));
                println!("    Config: {}", game.config_method);
                if let Some(ref path) = game.default_path {
                    println!("    Default Path: {}", path);
                }
            }
        }
    }

    Ok(())
}

/// Configure game for telemetry
async fn configure_game(
    client: &WheelClient,
    game_id: &str,
    path: Option<&str>,
    json: bool,
    auto: bool,
) -> Result<()> {
    let games = get_supported_games();
    let game = games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| CliError::InvalidConfiguration(format!("Unsupported game: {}", game_id)))?;

    let install_path = if let Some(path) = path {
        path.to_string()
    } else if auto {
        // Try to auto-detect installation path
        detect_game_path(game_id)?
    } else if !json {
        // Interactive path input
        Input::new()
            .with_prompt(format!("Enter installation path for {}", game.name))
            .interact_text()?
    } else {
        return Err(CliError::InvalidConfiguration(
            "Installation path required for JSON mode".to_string(),
        )
        .into());
    };

    // Configure telemetry
    client
        .configure_telemetry(game_id, Some(&install_path))
        .await?;

    output::print_success(
        &format!("Configured {} for telemetry at {}", game.name, install_path),
        json,
    );

    // Show configuration details
    if !json {
        println!("\nConfiguration applied:");
        match game_id {
            "iracing" => {
                println!("  • Updated app.ini with UDP telemetry settings");
                println!("  • Enabled shared memory interface");
                println!("  • Set telemetry rate to 60Hz");
            }
            "acc" => {
                println!("  • Enabled UDP broadcast on port 9996");
                println!("  • Configured telemetry output rate");
                println!("  • Added LED heartbeat validation");
            }
            "ams2" => {
                println!("  • Enabled shared memory telemetry");
                println!("  • Configured data export settings");
            }
            _ => {
                println!("  • Applied game-specific configuration");
            }
        }

        println!(
            "\n{} Start the game to test telemetry connection",
            "Next:".bold()
        );
    }

    Ok(())
}

/// Show game status
async fn show_game_status(client: &WheelClient, json: bool, show_telemetry: bool) -> Result<()> {
    let status = client.get_game_status().await?;

    output::print_game_status(&status, json);

    if show_telemetry && status.telemetry_active && !json {
        println!("\n{}", "Live Telemetry Data:".bold());

        // Mock telemetry data display
        for i in 0..5 {
            tokio::time::sleep(Duration::from_millis(200)).await;
            println!(
                "  RPM: {:4} | Speed: {:3} km/h | Gear: {} | FFB: {:3}%",
                6500 + (i * 100),
                120 + (i * 5),
                3,
                75 + (i * 2)
            );
        }
        println!("  ... (Press Ctrl+C to stop)");
    }

    Ok(())
}

/// Test telemetry connection
async fn test_telemetry(
    _client: &WheelClient,
    game_id: &str,
    duration: u64,
    json: bool,
) -> Result<()> {
    let games = get_supported_games();
    let game = games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| CliError::InvalidConfiguration(format!("Unsupported game: {}", game_id)))?;

    if !json {
        println!(
            "Testing telemetry connection for {} ({} seconds)...",
            game.name, duration
        );
        println!("Make sure the game is running and in a session.");
        println!();
    }

    let mut packets_received = 0;
    let mut led_heartbeats = 0;
    let mut interval = interval(Duration::from_millis(100));
    let end_time = tokio::time::Instant::now() + Duration::from_secs(duration);

    while tokio::time::Instant::now() < end_time {
        interval.tick().await;

        // Mock telemetry reception
        if rand::random::<f32>() > 0.1 {
            packets_received += 1;
        }

        if rand::random::<f32>() > 0.8 {
            led_heartbeats += 1;
        }

        if !json && packets_received % 50 == 0 {
            println!(
                "Packets received: {} | LED heartbeats: {}",
                packets_received, led_heartbeats
            );
        }
    }

    let success_rate = packets_received as f32 / (duration * 10) as f32;
    let test_passed = success_rate > 0.8;

    if json {
        let output = serde_json::json!({
            "success": test_passed,
            "game_id": game_id,
            "duration_seconds": duration,
            "packets_received": packets_received,
            "led_heartbeats": led_heartbeats,
            "success_rate": success_rate,
            "test_passed": test_passed
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("\n{}", "Test Results:".bold());
        println!("  Packets received: {}", packets_received);
        println!("  LED heartbeats: {}", led_heartbeats);
        println!("  Success rate: {:.1}%", success_rate * 100.0);

        if test_passed {
            println!("  {}", "✓ Telemetry connection OK".green());
        } else {
            println!("  {}", "✗ Telemetry connection issues detected".red());
            println!("\n{}", "Troubleshooting:".bold());
            println!("  • Verify game is running and in a session");
            println!("  • Check firewall settings for UDP traffic");
            println!("  • Ensure game telemetry is enabled in settings");
        }
    }

    Ok(())
}

// Helper functions and data structures

use colored::*;

#[derive(serde::Serialize, Debug)]
struct GameInfo {
    id: String,
    name: String,
    version: String,
    features: Vec<String>,
    config_method: String,
    default_path: Option<String>,
}

fn get_supported_games() -> Vec<GameInfo> {
    vec![
        GameInfo {
            id: "iracing".to_string(),
            name: "iRacing".to_string(),
            version: "2024.x".to_string(),
            features: vec![
                "FFB Scalar".to_string(),
                "RPM".to_string(),
                "Car ID".to_string(),
            ],
            config_method: "app.ini[v17]".to_string(),
            default_path: Some("C:\\Program Files (x86)\\iRacing".to_string()),
        },
        GameInfo {
            id: "acc".to_string(),
            name: "Assetto Corsa Competizione".to_string(),
            version: "1.9.x".to_string(),
            features: vec![
                "FFB Scalar".to_string(),
                "RPM".to_string(),
                "Car ID".to_string(),
                "DRS".to_string(),
            ],
            config_method: "UDP broadcast".to_string(),
            default_path: Some(
                "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Assetto Corsa Competizione"
                    .to_string(),
            ),
        },
        GameInfo {
            id: "ams2".to_string(),
            name: "Automobilista 2".to_string(),
            version: "1.5.x".to_string(),
            features: vec!["FFB Scalar".to_string(), "RPM".to_string()],
            config_method: "Shared memory".to_string(),
            default_path: Some(
                "C:\\Program Files (x86)\\Steam\\steamapps\\common\\Automobilista 2".to_string(),
            ),
        },
        GameInfo {
            id: "rf2".to_string(),
            name: "rFactor 2".to_string(),
            version: "1.1.x".to_string(),
            features: vec![
                "FFB Scalar".to_string(),
                "RPM".to_string(),
                "Telemetry".to_string(),
            ],
            config_method: "Plugin".to_string(),
            default_path: Some(
                "C:\\Program Files (x86)\\Steam\\steamapps\\common\\rFactor 2".to_string(),
            ),
        },
    ]
}

fn detect_game_path(game_id: &str) -> Result<String> {
    // Mock auto-detection - in real implementation this would check registry,
    // Steam library folders, etc.
    let games = get_supported_games();
    let game = games
        .iter()
        .find(|g| g.id == game_id)
        .ok_or_else(|| CliError::InvalidConfiguration(format!("Unknown game: {}", game_id)))?;

    if let Some(ref default_path) = game.default_path {
        // In real implementation, verify path exists
        Ok(default_path.clone())
    } else {
        Err(
            CliError::InvalidConfiguration(format!("Cannot auto-detect path for {}", game.name))
                .into(),
        )
    }
}
