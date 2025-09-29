//! Health monitoring commands

use anyhow::Result;
use std::time::Duration;
use tokio::time::timeout;

use crate::client::WheelClient;

use crate::output;

/// Execute health monitoring
pub async fn execute(watch: bool, json: bool, endpoint: Option<&str>) -> Result<()> {
    let client = WheelClient::connect(endpoint).await?;
    
    if watch {
        watch_health_events(&client, json).await
    } else {
        show_health_snapshot(&client, json).await
    }
}

/// Show current health snapshot
async fn show_health_snapshot(client: &WheelClient, json: bool) -> Result<()> {
    let devices = client.list_devices().await?;
    
    if devices.is_empty() {
        if json {
            let output = serde_json::json!({
                "success": true,
                "service_status": "running",
                "devices": [],
                "overall_health": "no_devices"
            });
            println!("{}", serde_json::to_string_pretty(&output)?);
        } else {
            println!("{}", "Service Health Status".bold());
            println!("  Service: {}", "Running".green());
            println!("  Devices: {}", "None connected".yellow());
        }
        return Ok(());
    }
    
    let mut device_health = Vec::new();
    let mut overall_healthy = true;
    
    for device in &devices {
        match client.get_device_status(&device.id).await {
            Ok(status) => {
                let healthy = status.active_faults.is_empty() && 
                            status.telemetry.temp_c < 80;
                
                if !healthy {
                    overall_healthy = false;
                }
                
                device_health.push(serde_json::json!({
                    "device_id": device.id,
                    "name": device.name,
                    "healthy": healthy,
                    "faults": status.active_faults,
                    "temperature": status.telemetry.temp_c,
                    "last_seen": status.last_seen
                }));
            }
            Err(_) => {
                overall_healthy = false;
                device_health.push(serde_json::json!({
                    "device_id": device.id,
                    "name": device.name,
                    "healthy": false,
                    "error": "Communication failed"
                }));
            }
        }
    }
    
    if json {
        let output = serde_json::json!({
            "success": true,
            "service_status": "running",
            "overall_health": if overall_healthy { "healthy" } else { "degraded" },
            "devices": device_health
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        println!("{}", "Service Health Status".bold());
        println!("  Service: {}", "Running".green());
        
        let health_status = if overall_healthy { 
            "Healthy".green() 
        } else { 
            "Degraded".yellow() 
        };
        println!("  Overall: {}", health_status);
        println!("  Devices: {}", devices.len());
        
        for device in device_health {
            let device_name = device["name"].as_str().unwrap_or("Unknown");
            let device_id = device["device_id"].as_str().unwrap_or("Unknown");
            let healthy = device["healthy"].as_bool().unwrap_or(false);
            
            let status_icon = if healthy { "✓".green() } else { "✗".red() };
            println!("    {} {} ({})", status_icon, device_name, device_id.dimmed());
            
            if let Some(faults) = device["faults"].as_array() {
                if !faults.is_empty() {
                    for fault in faults {
                        if let Some(fault_str) = fault.as_str() {
                            println!("      • {}", fault_str.red());
                        }
                    }
                }
            }
            
            if let Some(error) = device["error"].as_str() {
                println!("      • {}", error.red());
            }
        }
    }
    
    Ok(())
}

/// Watch health events in real-time
async fn watch_health_events(client: &WheelClient, json: bool) -> Result<()> {
    if !json {
        println!("Watching health events (Press Ctrl+C to stop)");
        println!("Timestamp    Device      Event                Message");
        println!("─────────────────────────────────────────────────────────────");
    }
    
    let mut health_stream = client.subscribe_health().await?;
    
    loop {
        // Use timeout to periodically check for new events
        match timeout(Duration::from_secs(1), health_stream.next()).await {
            Ok(Some(event)) => {
                output::print_health_event(&event, json);
            }
            Ok(None) => {
                // Stream ended
                if !json {
                    println!("Health event stream ended");
                }
                break;
            }
            Err(_) => {
                // Timeout - continue watching
                continue;
            }
        }
    }
    
    Ok(())
}

use colored::*;