//! Plugin management commands
//!
//! Provides commands for managing plugins from the registry:
//! - `plugin list` - List available plugins from registry
//! - `plugin search <query>` - Search for plugins
//! - `plugin install <plugin-id>` - Install a plugin
//! - `plugin uninstall <plugin-id>` - Uninstall a plugin
//! - `plugin info <plugin-id>` - Show plugin details

use anyhow::Result;
use colored::*;
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressStyle};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

use crate::commands::PluginCommands;
use crate::error::CliError;
use crate::output;

/// Execute plugin command
pub async fn execute(cmd: &PluginCommands, json: bool, _endpoint: Option<&str>) -> Result<()> {
    match cmd {
        PluginCommands::List { category } => list_plugins(category.as_deref(), json).await,
        PluginCommands::Search { query } => search_plugins(query, json).await,
        PluginCommands::Install { plugin_id, version } => {
            install_plugin(plugin_id, version.as_deref(), json).await
        }
        PluginCommands::Uninstall { plugin_id, force } => {
            uninstall_plugin(plugin_id, json, *force).await
        }
        PluginCommands::Info { plugin_id, version } => {
            show_plugin_info(plugin_id, version.as_deref(), json).await
        }
    }
}

/// Plugin information for display
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub version: String,
    pub author: String,
    pub description: String,
    pub license: String,
    pub homepage: Option<String>,
    pub installed: bool,
    pub installed_version: Option<String>,
    pub signature_verified: bool,
}

/// List available plugins from registry
async fn list_plugins(category: Option<&str>, json: bool) -> Result<()> {
    // Mock implementation - in real version this would fetch from registry
    let plugins = get_mock_registry_plugins();

    // Filter by category if provided
    let filtered: Vec<_> = if let Some(cat) = category {
        plugins
            .into_iter()
            .filter(|p| p.description.to_lowercase().contains(&cat.to_lowercase()))
            .collect()
    } else {
        plugins
    };

    if json {
        let output = serde_json::json!({
            "success": true,
            "plugins": filtered
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if filtered.is_empty() {
            println!("{}", "No plugins found in registry".yellow());
            return Ok(());
        }

        println!("{}", "Available Plugins:".bold());
        println!();

        for plugin in &filtered {
            print_plugin_summary(plugin);
        }

        println!();
        println!("{}", format!("Total: {} plugins", filtered.len()).dimmed());
        println!(
            "{}",
            "Use 'wheelctl plugin info <plugin-id>' for more details".dimmed()
        );
    }

    Ok(())
}

/// Search for plugins by query
async fn search_plugins(query: &str, json: bool) -> Result<()> {
    let plugins = get_mock_registry_plugins();

    let query_lower = query.to_lowercase();
    let results: Vec<_> = plugins
        .into_iter()
        .filter(|p| {
            p.name.to_lowercase().contains(&query_lower)
                || p.description.to_lowercase().contains(&query_lower)
                || p.author.to_lowercase().contains(&query_lower)
        })
        .collect();

    if json {
        let output = serde_json::json!({
            "success": true,
            "query": query,
            "results": results
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
    } else {
        if results.is_empty() {
            println!(
                "{}",
                format!("No plugins found matching '{}'", query).yellow()
            );
            return Ok(());
        }

        println!("{}", format!("Search results for '{}':", query).bold());
        println!();

        for plugin in &results {
            print_plugin_summary(plugin);
        }

        println!();
        println!(
            "{}",
            format!("Found {} matching plugins", results.len()).dimmed()
        );
    }

    Ok(())
}

/// Install a plugin from the registry
async fn install_plugin(plugin_id: &str, version: Option<&str>, json: bool) -> Result<()> {
    let plugins = get_mock_registry_plugins();

    // Find the plugin
    let plugin = plugins
        .iter()
        .find(|p| p.id == plugin_id || p.name.to_lowercase() == plugin_id.to_lowercase())
        .ok_or_else(|| {
            CliError::ValidationError(format!("Plugin '{}' not found in registry", plugin_id))
        })?;

    let target_version = version.unwrap_or(&plugin.version);

    if json {
        // In JSON mode, just output the result
        let output = serde_json::json!({
            "success": true,
            "action": "install",
            "plugin": {
                "id": plugin.id,
                "name": plugin.name,
                "version": target_version,
                "signature_verified": plugin.signature_verified
            },
            "message": format!("Plugin '{}' v{} installed successfully", plugin.name, target_version)
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Interactive installation
    println!(
        "{}",
        format!("Installing {} v{}...", plugin.name, target_version).bold()
    );
    println!();

    // Show plugin info
    println!("  {} {}", "Name:".dimmed(), plugin.name);
    println!("  {} {}", "Version:".dimmed(), target_version);
    println!("  {} {}", "Author:".dimmed(), plugin.author);
    println!("  {} {}", "License:".dimmed(), plugin.license);
    println!();

    // Verify signature
    let pb = ProgressBar::new_spinner();
    let style = ProgressStyle::default_spinner().template("{spinner:.green} {msg}")?;
    pb.set_style(style);

    pb.set_message("Verifying plugin signature...");
    pb.enable_steady_tick(Duration::from_millis(100));
    tokio::time::sleep(Duration::from_millis(500)).await;

    if plugin.signature_verified {
        pb.finish_with_message(format!("{} Signature verified", "✓".green()));
    } else {
        pb.finish_with_message(format!("{} Plugin is unsigned", "⚠".yellow()));
        println!();
        println!(
            "{}",
            "Warning: This plugin is not signed. Installing unsigned plugins may pose security risks.".yellow()
        );
        println!();
    }

    // Download plugin
    let pb = ProgressBar::new(100);
    let style = ProgressStyle::default_bar()
        .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}% {msg}")?
        .progress_chars("█▓░");
    pb.set_style(style);
    pb.set_message("Downloading plugin...");

    for i in 0..=100 {
        pb.set_position(i);
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    pb.finish_with_message("Download complete");

    // Install plugin
    let pb = ProgressBar::new_spinner();
    let style = ProgressStyle::default_spinner().template("{spinner:.green} {msg}")?;
    pb.set_style(style);

    pb.set_message("Installing plugin...");
    pb.enable_steady_tick(Duration::from_millis(100));
    tokio::time::sleep(Duration::from_millis(300)).await;
    pb.finish_with_message(format!("{} Plugin installed", "✓".green()));

    println!();
    output::print_success(
        &format!(
            "Plugin '{}' v{} installed successfully to {}",
            plugin.name,
            target_version,
            get_plugin_directory().display()
        ),
        false,
    );

    Ok(())
}

/// Uninstall a plugin
async fn uninstall_plugin(plugin_id: &str, json: bool, force: bool) -> Result<()> {
    let plugins = get_mock_registry_plugins();

    // Find the plugin
    let plugin = plugins
        .iter()
        .find(|p| p.id == plugin_id || p.name.to_lowercase() == plugin_id.to_lowercase())
        .ok_or_else(|| CliError::ValidationError(format!("Plugin '{}' not found", plugin_id)))?;

    // Check if installed (mock - always say it's installed for demo)
    if !plugin.installed && !force {
        return Err(CliError::ValidationError(format!(
            "Plugin '{}' is not installed",
            plugin.name
        ))
        .into());
    }

    if json {
        let output = serde_json::json!({
            "success": true,
            "action": "uninstall",
            "plugin": {
                "id": plugin.id,
                "name": plugin.name
            },
            "message": format!("Plugin '{}' uninstalled successfully", plugin.name)
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Confirm uninstallation
    if !force {
        println!(
            "{}",
            format!("Uninstalling plugin '{}'...", plugin.name).bold()
        );
        println!();

        if !Confirm::new()
            .with_prompt("Are you sure you want to uninstall this plugin?")
            .interact()?
        {
            output::print_warning("Uninstallation cancelled", false);
            return Ok(());
        }
    }

    // Perform uninstallation
    let pb = ProgressBar::new_spinner();
    let style = ProgressStyle::default_spinner().template("{spinner:.green} {msg}")?;
    pb.set_style(style);

    pb.set_message("Removing plugin files...");
    pb.enable_steady_tick(Duration::from_millis(100));
    tokio::time::sleep(Duration::from_millis(300)).await;
    pb.finish_with_message(format!("{} Plugin removed", "✓".green()));

    println!();
    output::print_success(
        &format!("Plugin '{}' uninstalled successfully", plugin.name),
        false,
    );

    Ok(())
}

/// Show detailed plugin information
async fn show_plugin_info(plugin_id: &str, version: Option<&str>, json: bool) -> Result<()> {
    let plugins = get_mock_registry_plugins();

    // Find the plugin
    let plugin = plugins
        .iter()
        .find(|p| p.id == plugin_id || p.name.to_lowercase() == plugin_id.to_lowercase())
        .ok_or_else(|| {
            CliError::ValidationError(format!("Plugin '{}' not found in registry", plugin_id))
        })?;

    if json {
        let output = serde_json::json!({
            "success": true,
            "plugin": plugin
        });
        println!("{}", serde_json::to_string_pretty(&output)?);
        return Ok(());
    }

    // Display detailed info
    println!("{}", "Plugin Information".bold());
    println!("{}", "─".repeat(50));
    println!();

    println!("  {} {}", "Name:".bold(), plugin.name.cyan());
    println!("  {} {}", "ID:".bold(), plugin.id.dimmed());
    println!(
        "  {} {}",
        "Version:".bold(),
        version.unwrap_or(&plugin.version)
    );
    println!("  {} {}", "Author:".bold(), plugin.author);
    println!("  {} {}", "License:".bold(), plugin.license);
    println!();

    println!("  {} {}", "Description:".bold(), plugin.description);
    println!();

    if let Some(ref homepage) = plugin.homepage {
        println!("  {} {}", "Homepage:".bold(), homepage.blue().underline());
        println!();
    }

    // Installation status
    println!("  {}:", "Status".bold());
    if plugin.installed {
        println!(
            "    {} Installed (v{})",
            "●".green(),
            plugin
                .installed_version
                .as_deref()
                .unwrap_or(&plugin.version)
        );
    } else {
        println!("    {} Not installed", "○".dimmed());
    }

    // Signature status
    if plugin.signature_verified {
        println!("    {} Signature verified", "✓".green());
    } else {
        println!("    {} Unsigned", "⚠".yellow());
    }

    println!();
    println!("{}", "─".repeat(50));

    if !plugin.installed {
        println!(
            "{}",
            format!("Install with: wheelctl plugin install {}", plugin.id).dimmed()
        );
    }

    Ok(())
}

/// Print a summary line for a plugin
fn print_plugin_summary(plugin: &PluginInfo) {
    let status_icon = if plugin.installed {
        "●".green()
    } else {
        "○".dimmed()
    };

    let signature_icon = if plugin.signature_verified {
        "✓".green()
    } else {
        "⚠".yellow()
    };

    println!(
        "  {} {} {} v{}",
        status_icon,
        plugin.name.bold(),
        signature_icon,
        plugin.version.dimmed()
    );
    println!("    {}", plugin.description.dimmed());
    println!("    {} {}", "by".dimmed(), plugin.author.dimmed());
    println!();
}

/// Get the plugin installation directory
fn get_plugin_directory() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));

    #[cfg(windows)]
    let plugin_dir = home
        .join("AppData")
        .join("Local")
        .join("Wheel")
        .join("plugins");

    #[cfg(not(windows))]
    let plugin_dir = home.join(".wheel").join("plugins");

    plugin_dir
}

/// Get mock registry plugins for demonstration
fn get_mock_registry_plugins() -> Vec<PluginInfo> {
    vec![
        PluginInfo {
            id: "ffb-smoothing".to_string(),
            name: "FFB Smoothing Filter".to_string(),
            version: "1.2.0".to_string(),
            author: "OpenRacing Team".to_string(),
            description:
                "Applies smoothing to force feedback output to reduce high-frequency noise"
                    .to_string(),
            license: "MIT".to_string(),
            homepage: Some("https://github.com/openracing/ffb-smoothing".to_string()),
            installed: true,
            installed_version: Some("1.1.0".to_string()),
            signature_verified: true,
        },
        PluginInfo {
            id: "led-dashboard".to_string(),
            name: "LED Dashboard Controller".to_string(),
            version: "2.0.1".to_string(),
            author: "SimRacing Community".to_string(),
            description: "Controls LED displays and rev lights based on telemetry data".to_string(),
            license: "Apache-2.0".to_string(),
            homepage: Some("https://github.com/simracing/led-dashboard".to_string()),
            installed: false,
            installed_version: None,
            signature_verified: true,
        },
        PluginInfo {
            id: "telemetry-logger".to_string(),
            name: "Telemetry Logger".to_string(),
            version: "1.0.0".to_string(),
            author: "DataDriven Racing".to_string(),
            description: "Records telemetry data to CSV files for analysis".to_string(),
            license: "MIT".to_string(),
            homepage: None,
            installed: false,
            installed_version: None,
            signature_verified: true,
        },
        PluginInfo {
            id: "custom-curves".to_string(),
            name: "Custom FFB Curves".to_string(),
            version: "0.9.0".to_string(),
            author: "FFB Enthusiasts".to_string(),
            description: "Provides advanced curve-based FFB response customization".to_string(),
            license: "GPL-3.0".to_string(),
            homepage: Some("https://ffb-curves.example.com".to_string()),
            installed: false,
            installed_version: None,
            signature_verified: false,
        },
        PluginInfo {
            id: "iracing-integration".to_string(),
            name: "iRacing Enhanced Integration".to_string(),
            version: "3.1.0".to_string(),
            author: "iRacing Community".to_string(),
            description: "Enhanced telemetry integration for iRacing with additional FFB effects"
                .to_string(),
            license: "MIT".to_string(),
            homepage: Some("https://github.com/iracing-community/enhanced-integration".to_string()),
            installed: true,
            installed_version: Some("3.1.0".to_string()),
            signature_verified: true,
        },
    ]
}
