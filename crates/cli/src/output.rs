//! Output formatting for CLI responses

use anyhow::Error;
use colored::*;
use serde_json::json;
use std::collections::HashMap;

use crate::client::*;
use racing_wheel_schemas::config::ProfileSchema;

/// Print error in JSON format
pub fn print_error_json(error: &Error) {
    let error_json = json!({
        "success": false,
        "error": {
            "message": error.to_string(),
            "type": error_type_name(error)
        }
    });
    println!("{}", serde_json::to_string_pretty(&error_json).unwrap());
}

/// Print error in human-readable format
pub fn print_error_human(error: &Error) {
    eprintln!("{} {}", "Error:".red().bold(), error);
    
    // Print error chain if available
    let mut source = error.source();
    while let Some(err) = source {
        eprintln!("  {} {}", "Caused by:".yellow(), err);
        source = err.source();
    }
}

/// Print device list in specified format
pub fn print_device_list(devices: &[DeviceInfo], json: bool, detailed: bool) {
    if json {
        let output = json!({
            "success": true,
            "devices": devices
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        if devices.is_empty() {
            println!("{}", "No devices found".yellow());
            return;
        }
        
        println!("{}", "Connected Devices:".bold());
        for device in devices {
            print_device_human(device, detailed);
        }
    }
}

/// Print single device in human format
fn print_device_human(device: &DeviceInfo, detailed: bool) {
    let state_color = match device.state {
        DeviceState::Connected => "green",
        DeviceState::Disconnected => "red", 
        DeviceState::Faulted => "red",
        DeviceState::Calibrating => "yellow",
    };
    
    println!("  {} {} ({})", 
        "●".color(state_color),
        device.name.bold(),
        device.id.dimmed()
    );
    
    if detailed {
        println!("    Type: {:?}", device.device_type);
        println!("    State: {:?}", device.state);
        if device.capabilities.max_torque_nm > 0.0 {
            println!("    Max Torque: {:.1} Nm", device.capabilities.max_torque_nm);
        }
        println!("    Capabilities: {}", format_capabilities(&device.capabilities));
    }
}

/// Format device capabilities as a string
fn format_capabilities(caps: &DeviceCapabilities) -> String {
    let mut features = Vec::new();
    
    if caps.supports_pid {
        features.push("PID");
    }
    if caps.supports_raw_torque_1khz {
        features.push("Raw Torque");
    }
    if caps.supports_health_stream {
        features.push("Health");
    }
    if caps.supports_led_bus {
        features.push("LEDs");
    }
    
    if features.is_empty() {
        "None".to_string()
    } else {
        features.join(", ")
    }
}

/// Print device status
pub fn print_device_status(status: &DeviceStatus, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "status": status
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("{} {}", "Device:".bold(), status.device.name);
        println!("  ID: {}", status.device.id);
        println!("  State: {:?}", status.device.state);
        println!("  Last Seen: {}", status.last_seen.format("%Y-%m-%d %H:%M:%S UTC"));
        
        if !status.active_faults.is_empty() {
            println!("  {} {}", "Active Faults:".red().bold(), status.active_faults.len());
            for fault in &status.active_faults {
                println!("    • {}", fault.red());
            }
        } else {
            println!("  {}", "No Active Faults".green());
        }
        
        println!("  {}:", "Telemetry".bold());
        let tel = &status.telemetry;
        println!("    Wheel Angle: {:.1}°", tel.wheel_angle_mdeg as f32 / 1000.0);
        println!("    Wheel Speed: {:.1} rad/s", tel.wheel_speed_mrad_s as f32 / 1000.0);
        println!("    Temperature: {}°C", tel.temp_c);
        println!("    Hands On: {}", if tel.hands_on { "Yes".green() } else { "No".red() });
        println!("    Sequence: {}", tel.sequence);
    }
}

/// Print profile information
pub fn print_profile(profile: &ProfileSchema, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "profile": profile
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("{} {}", "Profile Schema:".bold(), profile.schema);
        
        if let Some(ref game) = profile.scope.game {
            print!("  Scope: {}", game.cyan());
            if let Some(ref car) = profile.scope.car {
                print!(" > {}", car.cyan());
            }
            if let Some(ref track) = profile.scope.track {
                print!(" > {}", track.cyan());
            }
            println!();
        }
        
        println!("  {}:", "Base Settings".bold());
        println!("    FFB Gain: {:.1}%", profile.base.ffb_gain * 100.0);
        println!("    DOR: {}°", profile.base.dor_deg);
        println!("    Torque Cap: {:.1} Nm", profile.base.torque_cap_nm);
        
        println!("    {}:", "Filters".bold());
        let f = &profile.base.filters;
        println!("      Reconstruction: {}", f.reconstruction);
        println!("      Friction: {:.2}", f.friction);
        println!("      Damper: {:.2}", f.damper);
        println!("      Inertia: {:.2}", f.inertia);
        println!("      Slew Rate: {:.2}", f.slew_rate);
        
        if !f.notch_filters.is_empty() {
            println!("      Notch Filters:");
            for (i, notch) in f.notch_filters.iter().enumerate() {
                println!("        {}: {:.1} Hz, Q={:.1}, Gain={:.1} dB", 
                    i + 1, notch.hz, notch.q, notch.gain_db);
            }
        }
        
        if !f.curve_points.is_empty() {
            println!("      Curve Points: {} points", f.curve_points.len());
        }
        
        if profile.signature.is_some() {
            println!("  {}", "✓ Signed".green());
        } else {
            println!("  {}", "⚠ Unsigned".yellow());
        }
    }
}

/// Print diagnostics information
pub fn print_diagnostics(diag: &DiagnosticInfo, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "diagnostics": diag
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("{} {}", "Diagnostics for:".bold(), diag.device_id);
        
        println!("  {}:", "System Info".bold());
        for (key, value) in &diag.system_info {
            println!("    {}: {}", key, value);
        }
        
        println!("  {}:", "Performance Metrics".bold());
        let perf = &diag.performance;
        println!("    P99 Jitter: {:.2} μs", perf.p99_jitter_us);
        println!("    Missed Tick Rate: {:.4}%", perf.missed_tick_rate * 100.0);
        println!("    Total Ticks: {}", perf.total_ticks);
        println!("    Missed Ticks: {}", perf.missed_ticks);
        
        if !diag.recent_faults.is_empty() {
            println!("  {}:", "Recent Faults".red().bold());
            for fault in &diag.recent_faults {
                println!("    • {}", fault.red());
            }
        } else {
            println!("  {}", "No Recent Faults".green());
        }
    }
}

/// Print game status
pub fn print_game_status(status: &GameStatus, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "game_status": status
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("{}", "Game Status:".bold());
        
        match &status.active_game {
            Some(game) => {
                println!("  Active Game: {}", game.cyan());
                println!("  Telemetry: {}", 
                    if status.telemetry_active { "Active".green() } else { "Inactive".red() }
                );
                
                if let Some(ref car) = status.car_id {
                    println!("  Car: {}", car);
                }
                if let Some(ref track) = status.track_id {
                    println!("  Track: {}", track);
                }
            }
            None => {
                println!("  {}", "No active game detected".yellow());
            }
        }
    }
}

/// Print health event
pub fn print_health_event(event: &HealthEvent, json: bool) {
    if json {
        println!("{}", serde_json::to_string(&event).unwrap());
    } else {
        let event_color = match event.event_type {
            HealthEventType::DeviceConnected => "green",
            HealthEventType::DeviceDisconnected => "red",
            HealthEventType::FaultDetected => "red",
            HealthEventType::FaultCleared => "green",
            HealthEventType::PerformanceWarning => "yellow",
        };
        
        println!("{} [{}] {}: {}", 
            event.timestamp.format("%H:%M:%S").to_string().dimmed(),
            event.device_id.cyan(),
            format!("{:?}", event.event_type).color(event_color),
            event.message
        );
    }
}

/// Print success message
pub fn print_success(message: &str, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "message": message
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("{} {}", "✓".green(), message);
    }
}

/// Print warning message
pub fn print_warning(message: &str, json: bool) {
    if json {
        let output = json!({
            "success": true,
            "warning": message
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        println!("{} {}", "⚠".yellow(), message);
    }
}

/// Get error type name for JSON output
fn error_type_name(error: &Error) -> String {
    // Try to get the concrete error type name
    format!("{:?}", error).split('(').next().unwrap_or("Unknown").to_string()
}

/// Print table of data
pub fn print_table<T>(headers: &[&str], rows: &[Vec<T>], json: bool) 
where 
    T: std::fmt::Display + serde::Serialize,
{
    if json {
        let mut table_data = Vec::new();
        for row in rows {
            let mut row_map = HashMap::new();
            for (i, header) in headers.iter().enumerate() {
                if let Some(value) = row.get(i) {
                    row_map.insert(header.to_string(), json!(value));
                }
            }
            table_data.push(row_map);
        }
        
        let output = json!({
            "success": true,
            "data": table_data
        });
        println!("{}", serde_json::to_string_pretty(&output).unwrap());
    } else {
        // Simple table formatting for human output
        if rows.is_empty() {
            println!("{}", "No data".yellow());
            return;
        }
        
        // Print headers
        for (i, header) in headers.iter().enumerate() {
            if i > 0 { print!("  "); }
            print!("{}", header.bold());
        }
        println!();
        
        // Print separator
        for (i, header) in headers.iter().enumerate() {
            if i > 0 { print!("  "); }
            print!("{}", "-".repeat(header.len()));
        }
        println!();
        
        // Print rows
        for row in rows {
            for (i, value) in row.iter().enumerate() {
                if i > 0 { print!("  "); }
                print!("{}", value);
            }
            println!();
        }
    }
}