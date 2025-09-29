//! Profile management commands

use anyhow::Result;
use dialoguer::Editor;
use racing_wheel_schemas::config::{ProfileSchema, ProfileValidator};
use std::fs;
use std::path::{Path, PathBuf};

use crate::client::WheelClient;
use crate::commands::ProfileCommands;
use crate::error::CliError;
use crate::output;

/// Execute profile command
pub async fn execute(cmd: &ProfileCommands, json: bool, endpoint: Option<&str>) -> Result<()> {
    let client = WheelClient::connect(endpoint).await?;
    
    match cmd {
        ProfileCommands::List { game, car } => {
            list_profiles(game.as_deref(), car.as_deref(), json).await
        }
        ProfileCommands::Show { profile } => {
            show_profile(profile, json).await
        }
        ProfileCommands::Apply { device, profile, skip_validation } => {
            apply_profile(&client, device, profile, json, *skip_validation).await
        }
        ProfileCommands::Create { path, from, game, car } => {
            create_profile(path, from.as_deref(), game.as_deref(), car.as_deref(), json).await
        }
        ProfileCommands::Edit { profile, field, value } => {
            edit_profile(profile, field.as_deref(), value.as_deref(), json).await
        }
        ProfileCommands::Validate { path, detailed } => {
            validate_profile(path, json, *detailed).await
        }
        ProfileCommands::Export { profile, output, signed } => {
            export_profile(profile, output.as_deref(), json, *signed).await
        }
        ProfileCommands::Import { path, target, verify } => {
            import_profile(path, target.as_deref(), json, *verify).await
        }
    }
}

/// List available profiles
async fn list_profiles(game: Option<&str>, car: Option<&str>, json: bool) -> Result<()> {
    let profile_dir = get_profile_directory()?;
    let profiles = scan_profiles(&profile_dir, game, car)?;
    
    if json {
        let output = serde_json::json!({
            "success": true,
            "profiles": profiles
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if profiles.is_empty() {
            println!("No profiles found");
            return Ok(());
        }
        
        println!("Available Profiles:");
        for profile_info in profiles {
            println!("  {} {}", 
                "●".color("green"),
                profile_info.path.display().to_string().bold()
            );
            if let Some(ref scope) = profile_info.scope {
                if let Some(ref game) = scope.game {
                    print!("    Game: {}", game.cyan());
                    if let Some(ref car) = scope.car {
                        print!(" | Car: {}", car.cyan());
                    }
                    println!();
                }
            }
        }
    }
    
    Ok(())
}

/// Show profile details
async fn show_profile(profile_path: &str, json: bool) -> Result<()> {
    let path = resolve_profile_path(profile_path)?;
    let content = fs::read_to_string(&path)
        .map_err(|_| CliError::ProfileNotFound(profile_path.to_string()))?;
    
    let validator = ProfileValidator::new()?;
    let profile = validator.validate_json(&content)?;
    
    output::print_profile(&profile, json);
    Ok(())
}

/// Apply profile to device
async fn apply_profile(
    client: &WheelClient,
    device: &str,
    profile_path: &str,
    json: bool,
    skip_validation: bool
) -> Result<()> {
    // Load and validate profile
    let path = resolve_profile_path(profile_path)?;
    let content = fs::read_to_string(&path)
        .map_err(|_| CliError::ProfileNotFound(profile_path.to_string()))?;
    
    let profile = if skip_validation {
        serde_json::from_str(&content)?
    } else {
        let validator = ProfileValidator::new()?;
        validator.validate_json(&content)?
    };
    
    // Apply to device
    client.apply_profile(device, &profile).await?;
    
    output::print_success(
        &format!("Profile {} applied to device {}", profile_path, device),
        json
    );
    
    Ok(())
}

/// Create new profile
async fn create_profile(
    path: &str,
    from: Option<&str>,
    game: Option<&str>,
    car: Option<&str>,
    json: bool
) -> Result<()> {
    let profile = if let Some(base_path) = from {
        // Copy from existing profile
        let base_path = resolve_profile_path(base_path)?;
        let content = fs::read_to_string(&base_path)
            .map_err(|_| CliError::ProfileNotFound(base_path.display().to_string()))?;
        
        let validator = ProfileValidator::new()?;
        let mut profile = validator.validate_json(&content)?;
        
        // Update scope if provided
        if game.is_some() || car.is_some() {
            profile.scope.game = game.map(|s| s.to_string());
            profile.scope.car = car.map(|s| s.to_string());
        }
        
        profile
    } else {
        // Create default profile
        create_default_profile(game, car)
    };
    
    // Ensure directory exists
    if let Some(parent) = Path::new(path).parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Write profile
    let content = serde_json::to_string_pretty(&profile)?;
    fs::write(path, content)?;
    
    output::print_success(
        &format!("Profile created at {}", path),
        json
    );
    
    Ok(())
}

/// Edit profile interactively or with specific field/value
async fn edit_profile(
    profile_path: &str,
    field: Option<&str>,
    value: Option<&str>,
    json: bool
) -> Result<()> {
    let path = resolve_profile_path(profile_path)?;
    let content = fs::read_to_string(&path)
        .map_err(|_| CliError::ProfileNotFound(profile_path.to_string()))?;
    
    let validator = ProfileValidator::new()?;
    let mut profile = validator.validate_json(&content)?;
    
    if let (Some(field), Some(value)) = (field, value) {
        // Direct field edit
        edit_profile_field(&mut profile, field, value)?;
    } else if !json {
        // Interactive edit
        let new_content = Editor::new()
            .edit(&serde_json::to_string_pretty(&profile)?)?
            .ok_or_else(|| CliError::InvalidConfiguration("Edit cancelled".to_string()))?;
        
        profile = validator.validate_json(&new_content)?;
    } else {
        return Err(CliError::InvalidConfiguration(
            "Field and value required for JSON mode".to_string()
        ).into());
    }
    
    // Write back
    let new_content = serde_json::to_string_pretty(&profile)?;
    fs::write(&path, new_content)?;
    
    output::print_success(
        &format!("Profile {} updated", profile_path),
        json
    );
    
    Ok(())
}

/// Validate profile
async fn validate_profile(path: &str, json: bool, detailed: bool) -> Result<()> {
    let content = fs::read_to_string(path)
        .map_err(|_| CliError::ProfileNotFound(path.to_string()))?;
    
    let validator = ProfileValidator::new()?;
    match validator.validate_json(&content) {
        Ok(profile) => {
            if json {
                let output = serde_json::json!({
                    "success": true,
                    "valid": true,
                    "profile": if detailed { Some(&profile) } else { None }
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("✓ Profile is valid");
                if detailed {
                    output::print_profile(&profile, false);
                }
            }
        }
        Err(e) => {
            if json {
                let output = serde_json::json!({
                    "success": false,
                    "valid": false,
                    "error": e.to_string()
                });
                println!("{}", serde_json::to_string_pretty(&output)?);
            } else {
                println!("✗ Profile validation failed:");
                println!("  {}", e);
            }
            return Err(e.into());
        }
    }
    
    Ok(())
}

/// Export profile
async fn export_profile(
    profile_path: &str,
    output: Option<&str>,
    json: bool,
    signed: bool
) -> Result<()> {
    let path = resolve_profile_path(profile_path)?;
    let content = fs::read_to_string(&path)
        .map_err(|_| CliError::ProfileNotFound(profile_path.to_string()))?;
    
    let validator = ProfileValidator::new()?;
    let mut profile = validator.validate_json(&content)?;
    
    // Handle signing if requested
    if signed && profile.signature.is_none() {
        // In real implementation, this would sign the profile
        profile.signature = Some("mock-signature".to_string());
    }
    
    let export_content = serde_json::to_string_pretty(&profile)?;
    
    if let Some(output_path) = output {
        fs::write(output_path, export_content)?;
        output::print_success(
            &format!("Profile exported to {}", output_path),
            json
        );
    } else {
        println!("{}", export_content);
    }
    
    Ok(())
}

/// Import profile
async fn import_profile(
    path: &str,
    target: Option<&str>,
    json: bool,
    verify: bool
) -> Result<()> {
    let content = fs::read_to_string(path)
        .map_err(|_| CliError::ProfileNotFound(path.to_string()))?;
    
    let validator = ProfileValidator::new()?;
    let profile = validator.validate_json(&content)?;
    
    // Verify signature if requested
    if verify && profile.signature.is_none() {
        return Err(CliError::ValidationError(
            "Profile is not signed".to_string()
        ).into());
    }
    
    // Determine target path
    let target_path = if let Some(target) = target {
        PathBuf::from(target)
    } else {
        let profile_dir = get_profile_directory()?;
        let filename = Path::new(path).file_name()
            .ok_or_else(|| CliError::InvalidConfiguration("Invalid path".to_string()))?;
        profile_dir.join(filename)
    };
    
    // Ensure directory exists
    if let Some(parent) = target_path.parent() {
        fs::create_dir_all(parent)?;
    }
    
    // Write profile
    fs::write(&target_path, content)?;
    
    output::print_success(
        &format!("Profile imported to {}", target_path.display()),
        json
    );
    
    Ok(())
}

// Helper functions

use colored::*;

fn get_profile_directory() -> Result<PathBuf> {
    let home = dirs::home_dir()
        .ok_or_else(|| CliError::InvalidConfiguration("Cannot find home directory".to_string()))?;
    
    #[cfg(windows)]
    let profile_dir = home.join("AppData").join("Local").join("Wheel").join("profiles");
    
    #[cfg(not(windows))]
    let profile_dir = home.join(".wheel").join("profiles");
    
    Ok(profile_dir)
}

fn resolve_profile_path(profile_path: &str) -> Result<PathBuf> {
    let path = Path::new(profile_path);
    
    if path.is_absolute() || path.exists() {
        Ok(path.to_path_buf())
    } else {
        // Try relative to profile directory
        let profile_dir = get_profile_directory()?;
        let full_path = profile_dir.join(profile_path);
        
        if full_path.exists() {
            Ok(full_path)
        } else {
            // Try with .json extension
            let with_ext = if !profile_path.ends_with(".json") {
                profile_dir.join(format!("{}.json", profile_path))
            } else {
                full_path
            };
            
            if with_ext.exists() {
                Ok(with_ext)
            } else {
                Ok(Path::new(profile_path).to_path_buf())
            }
        }
    }
}

#[derive(serde::Serialize)]
struct ProfileInfo {
    path: PathBuf,
    scope: Option<racing_wheel_schemas::config::ProfileScope>,
}

fn scan_profiles(
    dir: &Path,
    game_filter: Option<&str>,
    car_filter: Option<&str>
) -> Result<Vec<ProfileInfo>> {
    let mut profiles = Vec::new();
    
    if !dir.exists() {
        return Ok(profiles);
    }
    
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        
        if path.is_file() && path.extension().map_or(false, |ext| ext == "json") {
            if let Ok(content) = fs::read_to_string(&path) {
                if let Ok(profile) = serde_json::from_str::<ProfileSchema>(&content) {
                    // Apply filters
                    let matches = match (game_filter, car_filter) {
                        (Some(game), Some(car)) => {
                            profile.scope.game.as_deref() == Some(game) &&
                            profile.scope.car.as_deref() == Some(car)
                        }
                        (Some(game), None) => {
                            profile.scope.game.as_deref() == Some(game)
                        }
                        (None, Some(car)) => {
                            profile.scope.car.as_deref() == Some(car)
                        }
                        (None, None) => true,
                    };
                    
                    if matches {
                        profiles.push(ProfileInfo {
                            path,
                            scope: Some(profile.scope),
                        });
                    }
                }
            }
        } else if path.is_dir() {
            // Recursively scan subdirectories
            profiles.extend(scan_profiles(&path, game_filter, car_filter)?);
        }
    }
    
    Ok(profiles)
}

fn create_default_profile(game: Option<&str>, car: Option<&str>) -> ProfileSchema {
    ProfileSchema {
        schema: "wheel.profile/1".to_string(),
        scope: racing_wheel_schemas::config::ProfileScope {
            game: game.map(|s| s.to_string()),
            car: car.map(|s| s.to_string()),
            track: None,
        },
        base: racing_wheel_schemas::config::BaseSettings {
            ffb_gain: 0.75,
            dor_deg: 900,
            torque_cap_nm: 8.0,
            filters: racing_wheel_schemas::config::FilterConfig {
                reconstruction: 4,
                friction: 0.12,
                damper: 0.18,
                inertia: 0.08,
                notch_filters: vec![],
                slew_rate: 0.85,
                curve_points: vec![
                    racing_wheel_schemas::config::CurvePoint { input: 0.0, output: 0.0 },
                    racing_wheel_schemas::config::CurvePoint { input: 1.0, output: 1.0 },
                ],
            },
        },
        leds: None,
        haptics: None,
        signature: None,
    }
}

fn edit_profile_field(profile: &mut ProfileSchema, field: &str, value: &str) -> Result<()> {
    match field {
        "base.ffbGain" => {
            profile.base.ffb_gain = value.parse()
                .map_err(|_| CliError::ValidationError("Invalid FFB gain value".to_string()))?;
        }
        "base.dorDeg" => {
            profile.base.dor_deg = value.parse()
                .map_err(|_| CliError::ValidationError("Invalid DOR value".to_string()))?;
        }
        "base.torqueCapNm" => {
            profile.base.torque_cap_nm = value.parse()
                .map_err(|_| CliError::ValidationError("Invalid torque cap value".to_string()))?;
        }
        "scope.game" => {
            profile.scope.game = if value.is_empty() { None } else { Some(value.to_string()) };
        }
        "scope.car" => {
            profile.scope.car = if value.is_empty() { None } else { Some(value.to_string()) };
        }
        _ => {
            return Err(CliError::ValidationError(
                format!("Unknown field: {}", field)
            ).into());
        }
    }
    
    Ok(())
}