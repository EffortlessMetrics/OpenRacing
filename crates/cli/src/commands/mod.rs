//! Command implementations for wheelctl CLI

pub mod device;
pub mod profile;
pub mod diag;
pub mod game;
pub mod safety;
pub mod health;

use clap::Subcommand;

#[derive(Subcommand)]
pub enum DeviceCommands {
    /// List all connected devices
    List {
        /// Show detailed device information
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Show device status and telemetry
    Status {
        /// Device ID or name
        device: String,
        /// Watch status in real-time
        #[arg(short, long)]
        watch: bool,
    },
    
    /// Calibrate device (center, DOR, pedals)
    Calibrate {
        /// Device ID or name
        device: String,
        /// Calibration type
        #[arg(value_enum)]
        calibration_type: CalibrationType,
        /// Skip interactive prompts
        #[arg(short, long)]
        yes: bool,
    },
    
    /// Reset device to safe state
    Reset {
        /// Device ID or name
        device: String,
        /// Force reset without confirmation
        #[arg(short, long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum ProfileCommands {
    /// List available profiles
    List {
        /// Filter by game
        #[arg(short, long)]
        game: Option<String>,
        /// Filter by car
        #[arg(short, long)]
        car: Option<String>,
    },
    
    /// Show profile details
    Show {
        /// Profile path or ID
        profile: String,
    },
    
    /// Apply profile to device
    Apply {
        /// Device ID or name
        device: String,
        /// Profile path or ID
        profile: String,
        /// Skip validation
        #[arg(long)]
        skip_validation: bool,
    },
    
    /// Create new profile
    Create {
        /// Profile file path
        path: String,
        /// Base profile to copy from
        #[arg(long)]
        from: Option<String>,
        /// Game scope
        #[arg(long)]
        game: Option<String>,
        /// Car scope
        #[arg(long)]
        car: Option<String>,
    },
    
    /// Edit profile interactively
    Edit {
        /// Profile path or ID
        profile: String,
        /// Field to edit (e.g., base.ffbGain)
        #[arg(long)]
        field: Option<String>,
        /// New value
        #[arg(long)]
        value: Option<String>,
    },
    
    /// Validate profile
    Validate {
        /// Profile path
        path: String,
        /// Show detailed validation info
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Export profile
    Export {
        /// Profile path or ID
        profile: String,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
        /// Include signature
        #[arg(long)]
        signed: bool,
    },
    
    /// Import profile
    Import {
        /// Profile file path
        path: String,
        /// Target directory
        #[arg(short, long)]
        target: Option<String>,
        /// Verify signature
        #[arg(long)]
        verify: bool,
    },
}

#[derive(Subcommand)]
pub enum DiagCommands {
    /// Run system diagnostics
    Test {
        /// Device ID or name
        #[arg(short, long)]
        device: Option<String>,
        /// Test type
        #[arg(value_enum)]
        test_type: Option<TestType>,
    },
    
    /// Record blackbox data
    Record {
        /// Device ID or name
        device: String,
        /// Recording duration in seconds
        #[arg(short, long, default_value = "120")]
        duration: u64,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
    
    /// Replay blackbox recording
    Replay {
        /// Blackbox file path
        file: String,
        /// Show frame-by-frame output
        #[arg(short, long)]
        verbose: bool,
    },
    
    /// Generate support bundle
    Support {
        /// Include blackbox recording
        #[arg(short, long)]
        blackbox: bool,
        /// Output file path
        #[arg(short, long)]
        output: Option<String>,
    },
    
    /// Show performance metrics
    Metrics {
        /// Device ID or name
        device: Option<String>,
        /// Watch metrics in real-time
        #[arg(short, long)]
        watch: bool,
    },
}

#[derive(Subcommand)]
pub enum GameCommands {
    /// List supported games
    List {
        /// Show configuration details
        #[arg(short, long)]
        detailed: bool,
    },
    
    /// Configure game for telemetry
    Configure {
        /// Game ID
        game: String,
        /// Game installation path
        #[arg(short, long)]
        path: Option<String>,
        /// Enable auto-configuration
        #[arg(long)]
        auto: bool,
    },
    
    /// Show game status
    Status {
        /// Show telemetry data
        #[arg(short, long)]
        telemetry: bool,
    },
    
    /// Test telemetry connection
    Test {
        /// Game ID
        game: String,
        /// Test duration in seconds
        #[arg(short, long, default_value = "10")]
        duration: u64,
    },
}

#[derive(Subcommand)]
pub enum SafetyCommands {
    /// Enable high torque mode
    Enable {
        /// Device ID or name
        device: String,
        /// Skip safety confirmation
        #[arg(long)]
        force: bool,
    },
    
    /// Emergency stop all devices
    Stop {
        /// Specific device ID or name
        device: Option<String>,
    },
    
    /// Show safety status
    Status {
        /// Device ID or name
        device: Option<String>,
    },
    
    /// Set torque limits
    Limit {
        /// Device ID or name
        device: String,
        /// Maximum torque in Nm
        torque: f32,
        /// Apply to all profiles
        #[arg(long)]
        global: bool,
    },
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum CalibrationType {
    Center,
    Dor,
    Pedals,
    All,
}

#[derive(clap::ValueEnum, Clone, Debug)]
pub enum TestType {
    Motor,
    Encoder,
    Usb,
    Thermal,
    All,
}