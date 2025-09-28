//! Racing Wheel Service Daemon (wheeld)

use racing_wheel_service::WheelService;
use tracing::{info, error};
use tracing_subscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter("racing_wheel_service=debug,info")
        .init();

    info!("Starting Racing Wheel Service v{}", env!("CARGO_PKG_VERSION"));

    // Create and start the service
    let service = WheelService::new().await?;
    
    // Set up signal handling for graceful shutdown
    let shutdown_signal = async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to install CTRL+C signal handler");
        info!("Received shutdown signal");
    };

    // Run service until shutdown
    tokio::select! {
        result = service.run() => {
            if let Err(e) = result {
                error!("Service error: {}", e);
                return Err(e);
            }
        }
        _ = shutdown_signal => {
            info!("Shutting down gracefully...");
        }
    }

    info!("Service stopped");
    Ok(())
}