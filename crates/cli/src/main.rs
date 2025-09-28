//! wheelctl - Racing Wheel Control CLI

use clap::{Parser, Subcommand};
use anyhow::Result;

#[derive(Parser)]
#[command(name = "wheelctl")]
#[command(about = "Racing Wheel Control CLI")]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// List connected devices
    List,
    /// Show device status
    Status {
        /// Device ID
        device_id: String,
    },
    /// Apply profile to device
    Profile {
        /// Device ID
        device_id: String,
        /// Profile file path
        profile: String,
    },
}

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::List => {
            println!("Listing devices...");
            // Device listing will be implemented in later tasks
        }
        Commands::Status { device_id } => {
            println!("Device status for: {}", device_id);
            // Status command will be implemented in later tasks
        }
        Commands::Profile { device_id, profile } => {
            println!("Applying profile {} to device {}", profile, device_id);
            // Profile application will be implemented in later tasks
        }
    }

    Ok(())
}