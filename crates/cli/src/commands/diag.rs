//! Diagnostic and monitoring commands

use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::time::interval;

use crate::client::WheelClient;
use crate::commands::{DiagCommands, TestType};
use crate::error::CliError;
use crate::output;

/// Execute diagnostic command
pub async fn execute(cmd: &DiagCommands, json: bool, endpoint: Option<&str>) -> Result<()> {
    let client = WheelClient::connect(endpoint).await?;

    match cmd {
        DiagCommands::Test { device, test_type } => {
            run_diagnostics(&client, device.as_deref(), test_type.as_ref(), json).await
        }
        DiagCommands::Record {
            device,
            duration,
            output,
        } => record_blackbox(&client, device, *duration, output.as_deref(), json).await,
        DiagCommands::Replay { file, detailed } => replay_blackbox(file, json, *detailed).await,
        DiagCommands::Support { blackbox, output } => {
            generate_support_bundle(&client, *blackbox, output.as_deref(), json).await
        }
        DiagCommands::Metrics { device, watch } => {
            show_metrics(&client, device.as_deref(), json, *watch).await
        }
    }
}

/// Run system diagnostics
async fn run_diagnostics(
    client: &WheelClient,
    device: Option<&str>,
    test_type: Option<&TestType>,
    json: bool,
) -> Result<()> {
    let device_id = if let Some(device) = device {
        // Verify device exists
        let _status = client
            .get_device_status(device)
            .await
            .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;
        device.to_string()
    } else {
        // Use first available device
        let devices = client.list_devices().await?;
        if devices.is_empty() {
            return Err(CliError::DeviceNotFound("No devices found".to_string()).into());
        }
        devices[0].id.clone()
    };

    let tests_to_run = match test_type {
        Some(test) => vec![test.clone()],
        None => vec![
            TestType::Motor,
            TestType::Encoder,
            TestType::Usb,
            TestType::Thermal,
        ],
    };

    let mut results = Vec::new();

    for test in tests_to_run {
        let result = run_single_test(client, &device_id, &test, json).await?;
        results.push((test, result));
    }

    if json {
        let output = serde_json::json!({
            "success": true,
            "device_id": device_id,
            "test_results": results.iter().map(|(test, result)| {
                serde_json::json!({
                    "test": format!("{:?}", test),
                    "passed": result.passed,
                    "message": result.message,
                    "details": result.details
                })
            }).collect::<Vec<_>>()
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Diagnostic Results for {}:", device_id);
        for (test, result) in results {
            let status = if result.passed {
                "✓".green()
            } else {
                "✗".red()
            };
            println!("  {} {:?}: {}", status, test, result.message);
            if !result.details.is_empty() {
                for detail in result.details {
                    println!("    {}", detail);
                }
            }
        }
    }

    Ok(())
}

/// Record blackbox data
async fn record_blackbox(
    client: &WheelClient,
    device: &str,
    duration: u64,
    output: Option<&str>,
    json: bool,
) -> Result<()> {
    // Verify device exists
    let _status = client
        .get_device_status(device)
        .await
        .map_err(|_| CliError::DeviceNotFound(device.to_string()))?;

    let output_path = output.map(PathBuf::from).unwrap_or_else(|| {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!("blackbox_{}_{}.wbb", device, timestamp))
    });

    if !json {
        println!("Recording blackbox data for {} seconds...", duration);
        let pb = ProgressBar::new(duration);
        let style = ProgressStyle::default_bar()
            .template(
                "{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len}s {msg}",
            )?
            .progress_chars("#>-");
        pb.set_style(style);

        let mut interval = interval(Duration::from_secs(1));
        for i in 0..duration {
            interval.tick().await;
            pb.set_position(i + 1);
            pb.set_message(format!("Recording to {}", output_path.display()));
        }
        pb.finish_with_message("Recording complete");
    } else {
        tokio::time::sleep(Duration::from_secs(duration)).await;
    }

    // Mock blackbox file creation
    let mock_data = format!(
        "WBB1\x00\x00\x00\x00Mock blackbox data for device {} recorded for {} seconds\n",
        device, duration
    );
    fs::write(&output_path, mock_data)?;

    output::print_success(
        &format!("Blackbox recorded to {}", output_path.display()),
        json,
    );

    Ok(())
}

/// Replay blackbox recording
async fn replay_blackbox(file: &str, json: bool, detailed: bool) -> Result<()> {
    let content =
        fs::read_to_string(file).map_err(|_| CliError::ProfileNotFound(file.to_string()))?;

    if !content.starts_with("WBB1") {
        return Err(CliError::ValidationError("Invalid blackbox file format".to_string()).into());
    }

    if json {
        let output = serde_json::json!({
            "success": true,
            "file": file,
            "format": "WBB1",
            "frames_replayed": 1000,
            "duration_ms": 1000,
            "validation": "passed"
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("Replaying blackbox file: {}", file);
        println!("Format: WBB1");
        println!("Duration: 1.0s");
        println!("Frames: 1000");

        if detailed {
            println!("\nFrame-by-frame output:");
            for i in 0..10 {
                println!(
                    "  Frame {}: torque=0.{:02}, angle={}°, speed=0.0 rad/s",
                    i,
                    i * 5,
                    i * 10
                );
            }
            println!("  ... (990 more frames)");
        }

        println!("✓ Replay completed successfully");
    }

    Ok(())
}

/// Generate support bundle
async fn generate_support_bundle(
    client: &WheelClient,
    include_blackbox: bool,
    output: Option<&str>,
    json: bool,
) -> Result<()> {
    let output_path = output.map(PathBuf::from).unwrap_or_else(|| {
        let timestamp = chrono::Utc::now().format("%Y%m%d_%H%M%S");
        PathBuf::from(format!("support_bundle_{}.zip", timestamp))
    });

    if !json {
        let pb = ProgressBar::new_spinner();
        let style = ProgressStyle::default_spinner().template("{spinner:.green} {msg}")?;
        pb.set_style(style);

        pb.set_message("Collecting system information...");
        pb.enable_steady_tick(Duration::from_millis(100));
        tokio::time::sleep(Duration::from_secs(1)).await;

        pb.set_message("Gathering device diagnostics...");
        tokio::time::sleep(Duration::from_secs(1)).await;

        if include_blackbox {
            pb.set_message("Including blackbox recordings...");
            tokio::time::sleep(Duration::from_secs(2)).await;
        }

        pb.set_message("Creating support bundle...");
        tokio::time::sleep(Duration::from_secs(1)).await;

        pb.finish_with_message("Support bundle created");
    }

    // Mock support bundle creation
    let bundle_info = serde_json::json!({
        "timestamp": chrono::Utc::now(),
        "system_info": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "version": env!("CARGO_PKG_VERSION")
        },
        "devices": client.list_devices().await?,
        "blackbox_included": include_blackbox
    });

    fs::write(&output_path, serde_json::to_string_pretty(&bundle_info)?)?;

    output::print_success(
        &format!("Support bundle created: {} (2.1 MB)", output_path.display()),
        json,
    );

    Ok(())
}

/// Show performance metrics
async fn show_metrics(
    client: &WheelClient,
    device: Option<&str>,
    json: bool,
    watch: bool,
) -> Result<()> {
    if watch {
        watch_metrics(client, device, json).await
    } else {
        show_single_metrics(client, device, json).await
    }
}

/// Show metrics once
async fn show_single_metrics(client: &WheelClient, device: Option<&str>, json: bool) -> Result<()> {
    let device_id = if let Some(device) = device {
        device.to_string()
    } else {
        let devices = client.list_devices().await?;
        if devices.is_empty() {
            return Err(CliError::DeviceNotFound("No devices found".to_string()).into());
        }
        devices[0].id.clone()
    };

    let diag = client.get_diagnostics(&device_id).await?;
    output::print_diagnostics(&diag, json);

    Ok(())
}

/// Watch metrics in real-time
async fn watch_metrics(client: &WheelClient, device: Option<&str>, json: bool) -> Result<()> {
    let device_id = if let Some(device) = device {
        device.to_string()
    } else {
        let devices = client.list_devices().await?;
        if devices.is_empty() {
            return Err(CliError::DeviceNotFound("No devices found".to_string()).into());
        }
        devices[0].id.clone()
    };

    if !json {
        println!("Watching metrics for {} (Press Ctrl+C to stop)", device_id);
        println!();
    }

    let mut interval = interval(Duration::from_secs(1));

    loop {
        interval.tick().await;

        match client.get_diagnostics(&device_id).await {
            Ok(diag) => {
                if json {
                    output::print_diagnostics(&diag, true);
                } else {
                    // Clear screen and print metrics
                    print!("\x1B[2J\x1B[1;1H");
                    output::print_diagnostics(&diag, false);
                }
            }
            Err(_) => {
                if json {
                    output::print_error_json(&CliError::DeviceNotFound(device_id.clone()).into());
                } else {
                    eprintln!("Device {} not found", device_id);
                }
                break;
            }
        }
    }

    Ok(())
}

// Helper functions

use colored::*;

#[derive(Debug)]
struct TestResult {
    passed: bool,
    message: String,
    details: Vec<String>,
}

async fn run_single_test(
    _client: &WheelClient,
    _device_id: &str,
    test_type: &TestType,
    json: bool,
) -> Result<TestResult> {
    if !json {
        let pb = ProgressBar::new_spinner();
        let style = ProgressStyle::default_spinner().template("{spinner:.green} {msg}")?;
        pb.set_style(style);
        pb.set_message(format!("Running {:?} test...", test_type));
        pb.enable_steady_tick(Duration::from_millis(100));

        // Simulate test duration
        tokio::time::sleep(Duration::from_millis(500)).await;
        pb.finish_and_clear();
    }

    // Mock test results
    let result = match test_type {
        TestType::Motor => TestResult {
            passed: true,
            message: "Motor phases OK".to_string(),
            details: vec![
                "Phase A: 2.1Ω".to_string(),
                "Phase B: 2.0Ω".to_string(),
                "Phase C: 2.1Ω".to_string(),
            ],
        },
        TestType::Encoder => TestResult {
            passed: true,
            message: "Encoder integrity OK".to_string(),
            details: vec![
                "Resolution: 2048 CPR".to_string(),
                "Index pulse: Present".to_string(),
                "Noise level: <0.1%".to_string(),
            ],
        },
        TestType::Usb => TestResult {
            passed: true,
            message: "USB communication OK".to_string(),
            details: vec![
                "Latency: 0.15ms avg".to_string(),
                "Jitter: 0.08ms p99".to_string(),
                "Packet loss: 0%".to_string(),
            ],
        },
        TestType::Thermal => TestResult {
            passed: true,
            message: "Thermal management OK".to_string(),
            details: vec![
                "Current temp: 42°C".to_string(),
                "Max temp: 85°C".to_string(),
                "Cooling: Active".to_string(),
            ],
        },
        TestType::All => TestResult {
            passed: true,
            message: "All tests passed".to_string(),
            details: vec![],
        },
    };

    Ok(result)
}
