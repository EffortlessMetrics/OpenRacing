//! Racing Wheel Service Daemon (wheeld)

use racing_wheel_service::{WheelService, ServiceDaemon, ServiceConfig};
use tracing::{info, error, warn};
use tracing_subscriber;
use std::env;
use std::process;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("racing_wheel_service=debug,info")
        .init();

    info!("Starting Racing Wheel Service v{}", env!("CARGO_PKG_VERSION"));

    // Parse command line arguments
    let args: Vec<String> = env::args().collect();
    
    // Handle service installation/uninstallation commands
    if args.len() > 1 {
        match args[1].as_str() {
            "install" => {
                info!("Installing service...");
                if let Err(e) = ServiceDaemon::install().await {
                    error!("Failed to install service: {}", e);
                    process::exit(1);
                }
                info!("Service installed successfully");
                return Ok(());
            }
            "uninstall" => {
                info!("Uninstalling service...");
                if let Err(e) = ServiceDaemon::uninstall().await {
                    error!("Failed to uninstall service: {}", e);
                    process::exit(1);
                }
                info!("Service uninstalled successfully");
                return Ok(());
            }
            "status" => {
                match ServiceDaemon::status().await {
                    Ok(status) => {
                        info!("Service status: {}", status);
                        return Ok(());
                    }
                    Err(e) => {
                        error!("Failed to get service status: {}", e);
                        process::exit(1);
                    }
                }
            }
            _ => {
                warn!("Unknown command: {}. Use 'install', 'uninstall', or 'status'", args[1]);
            }
        }
    }

    // Load service configuration
    let config = ServiceConfig::load().await.unwrap_or_else(|e| {
        warn!("Failed to load config, using defaults: {}", e);
        ServiceConfig::default()
    });

    // Create service daemon
    let daemon = ServiceDaemon::new(config).await?;
    
    // Run the daemon
    if let Err(e) = daemon.run().await {
        error!("Service daemon error: {}", e);
        process::exit(1);
    }

    info!("Service stopped");
    Ok(())
}