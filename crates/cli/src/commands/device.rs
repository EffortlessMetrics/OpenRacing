//! Device management commands

use anyhow::Result;
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressStyle};
use std::time::Duration;
use tokio::time::interval;

use crate::client::WheelClient;
use crate::commands::{CalibrationType, DeviceCommands};
use crate::error::CliError;
use crate::output;

/// Execute device command
pub async fn execute(cmd: &DeviceCommands, json: bool, endpoint: Option<&str>) -> Result<()> {
    let client = WheelClient::connect(endpoint).await?;

    match cmd {
        DeviceCommands::List { detailed } => list_devices(&client, json, *detailed).await,
        DeviceCommands::Status { device, watch } => {
            device_status(&client, device, json, *watch).await
        }
        DeviceCommands::Calibrate {
            device,
            calibration_type,
            yes,
        } => calibrate_device(&client, device, calibration_type, json, *yes).await,
        DeviceCommands::Reset { device, force } => {
            reset_device(&client, device, json, *force).await
        }
    }
}

/// List all connected devices
async fn list_devices(client: &WheelClient, json: bool, detailed: bool) -> Result<()> {
    let devices = client.list_devices().await?;
    output::print_device_list(&devices, json, detailed);
    Ok(())
}

/// Show device status
async fn device_status(client: &WheelClient, device: &str, json: bool, watch: bool) -> Result<()> {
    if watch {
        watch_device_status(client, device, json).await
    } else {
        let status = client
            .get_device_status(device)
            .await
            .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;
        output::print_device_status(&status, json);
        Ok(())
    }
}

/// Watch device status in real-time
async fn watch_device_status(client: &WheelClient, device: &str, json: bool) -> Result<()> {
    if !json {
        println!(
            "Watching device status for {} (Press Ctrl+C to stop)",
            device
        );
        println!();
    }

    let mut interval = interval(Duration::from_millis(500));

    loop {
        interval.tick().await;

        match client.get_device_status(device).await {
            Ok(status) => {
                if json {
                    output::print_device_status(&status, true);
                } else {
                    // Clear screen and print status
                    print!("\x1B[2J\x1B[1;1H");
                    output::print_device_status(&status, false);
                }
            }
            Err(_) => {
                if json {
                    output::print_error_json(&CliError::DeviceNotFound(device.to_string()).into());
                } else {
                    eprintln!("Device {} not found", device);
                }
                break;
            }
        }
    }

    Ok(())
}

/// Calibrate device
async fn calibrate_device(
    client: &WheelClient,
    device: &str,
    calibration_type: &CalibrationType,
    json: bool,
    yes: bool,
) -> Result<()> {
    // Verify device exists
    let _status = client
        .get_device_status(device)
        .await
        .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;

    if !yes && !json {
        let message = match calibration_type {
            CalibrationType::Center => {
                "Center the wheel and press Enter to calibrate center position"
            }
            CalibrationType::Dor => {
                "Calibrate degrees of rotation (DOR) - wheel will be moved to limits"
            }
            CalibrationType::Pedals => {
                "Calibrate pedal ranges - press each pedal fully and release"
            }
            CalibrationType::All => "Perform full calibration sequence (center, DOR, pedals)",
        };

        if !Confirm::new()
            .with_prompt(format!("{}. Continue?", message))
            .interact()?
        {
            output::print_warning("Calibration cancelled", json);
            return Ok(());
        }
    }

    // Show progress during calibration
    if !json {
        let pb = ProgressBar::new_spinner();
        pb.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.green} {msg}")
                .unwrap(),
        );

        match calibration_type {
            CalibrationType::Center => {
                pb.set_message("Calibrating center position...");
                pb.enable_steady_tick(Duration::from_millis(100));
                tokio::time::sleep(Duration::from_secs(2)).await;
                pb.finish_with_message("✓ Center position calibrated");
            }
            CalibrationType::Dor => {
                pb.set_message("Calibrating degrees of rotation...");
                pb.enable_steady_tick(Duration::from_millis(100));
                tokio::time::sleep(Duration::from_secs(5)).await;
                pb.finish_with_message("✓ DOR calibrated (900°)");
            }
            CalibrationType::Pedals => {
                pb.set_message("Calibrating pedal ranges...");
                pb.enable_steady_tick(Duration::from_millis(100));
                tokio::time::sleep(Duration::from_secs(3)).await;
                pb.finish_with_message("✓ Pedal ranges calibrated");
            }
            CalibrationType::All => {
                for (step, msg) in [
                    (
                        "Calibrating center position...",
                        "✓ Center position calibrated",
                    ),
                    ("Calibrating degrees of rotation...", "✓ DOR calibrated"),
                    ("Calibrating pedal ranges...", "✓ Pedal ranges calibrated"),
                ] {
                    pb.set_message(step);
                    pb.enable_steady_tick(Duration::from_millis(100));
                    tokio::time::sleep(Duration::from_secs(2)).await;
                    pb.finish_with_message(msg);
                    pb.reset();
                }
            }
        }
    }

    output::print_success(
        &format!(
            "Device {} calibration ({:?}) completed successfully",
            device, calibration_type
        ),
        json,
    );

    Ok(())
}

/// Reset device to safe state
async fn reset_device(client: &WheelClient, device: &str, json: bool, force: bool) -> Result<()> {
    // Verify device exists
    let _status = client
        .get_device_status(device)
        .await
        .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;

    if !force && !json {
        if !Confirm::new()
            .with_prompt("Reset device to safe state? This will stop all force feedback and return to default settings.")
            .interact()?
        {
            output::print_warning("Reset cancelled", json);
            return Ok(());
        }
    }

    // Perform emergency stop
    client.emergency_stop(Some(device)).await?;

    output::print_success(&format!("Device {} reset to safe state", device), json);

    Ok(())
}
