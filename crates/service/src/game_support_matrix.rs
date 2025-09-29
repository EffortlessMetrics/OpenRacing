//! Game Support Matrix implementation

use crate::game_service::*;
use std::collections::HashMap;

impl GameSupportMatrix {
    /// Create default support matrix with iRacing and ACC
    pub fn create_default() -> Self {
        let mut games = HashMap::new();
        
        // iRacing support
        games.insert("iracing".to_string(), GameSupport {
            name: "iRacing".to_string(),
            versions: vec![
                GameVersion {
                    version: "2024.x".to_string(),
                    config_paths: vec![
                        "Documents/iRacing/app.ini".to_string(),
                    ],
                    executable_patterns: vec![
                        "iRacingSim64DX11.exe".to_string(),
                        "iRacingService.exe".to_string(),
                    ],
                    telemetry_method: "shared_memory".to_string(),
                    supported_fields: vec![
                        "ffb_scalar".to_string(),
                        "rpm".to_string(),
                        "speed_ms".to_string(),
                        "slip_ratio".to_string(),
                        "gear".to_string(),
                        "car_id".to_string(),
                        "track_id".to_string(),
                    ],
                },
            ],
            telemetry: TelemetrySupport {
                method: "shared_memory".to_string(),
                update_rate_hz: 60,
                fields: TelemetryFieldMapping {
                    ffb_scalar: Some("SteeringWheelTorque".to_string()),
                    rpm: Some("RPM".to_string()),
                    speed_ms: Some("Speed".to_string()),
                    slip_ratio: Some("LFslipRatio".to_string()),
                    gear: Some("Gear".to_string()),
                    flags: Some("SessionFlags".to_string()),
                    car_id: Some("CarIdx".to_string()),
                    track_id: Some("TrackId".to_string()),
                },
            },
            config_writer: "iracing".to_string(),
            auto_detect: AutoDetectConfig {
                process_names: vec![
                    "iRacingSim64DX11.exe".to_string(),
                    "iRacingService.exe".to_string(),
                ],
                install_registry_keys: vec![
                    "HKEY_CURRENT_USER\\Software\\iRacing.com\\iRacing".to_string(),
                ],
                install_paths: vec![
                    "Program Files (x86)/iRacing".to_string(),
                ],
            },
        });
        
        // ACC support
        games.insert("acc".to_string(), GameSupport {
            name: "Assetto Corsa Competizione".to_string(),
            versions: vec![
                GameVersion {
                    version: "1.9.x".to_string(),
                    config_paths: vec![
                        "Documents/Assetto Corsa Competizione/Config/broadcasting.json".to_string(),
                    ],
                    executable_patterns: vec![
                        "AC2-Win64-Shipping.exe".to_string(),
                    ],
                    telemetry_method: "udp_broadcast".to_string(),
                    supported_fields: vec![
                        "ffb_scalar".to_string(),
                        "rpm".to_string(),
                        "speed_ms".to_string(),
                        "slip_ratio".to_string(),
                        "gear".to_string(),
                        "car_id".to_string(),
                        "track_id".to_string(),
                    ],
                },
            ],
            telemetry: TelemetrySupport {
                method: "udp_broadcast".to_string(),
                update_rate_hz: 100,
                fields: TelemetryFieldMapping {
                    ffb_scalar: Some("steerAngle".to_string()),
                    rpm: Some("rpms".to_string()),
                    speed_ms: Some("speedKmh".to_string()),
                    slip_ratio: Some("wheelSlip".to_string()),
                    gear: Some("gear".to_string()),
                    flags: Some("flag".to_string()),
                    car_id: Some("carModel".to_string()),
                    track_id: Some("track".to_string()),
                },
            },
            config_writer: "acc".to_string(),
            auto_detect: AutoDetectConfig {
                process_names: vec![
                    "AC2-Win64-Shipping.exe".to_string(),
                ],
                install_registry_keys: vec![
                    "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Steam App 805550".to_string(),
                ],
                install_paths: vec![
                    "Program Files (x86)/Steam/steamapps/common/Assetto Corsa Competizione".to_string(),
                ],
            },
        });
        
        Self { games }
    }
}

impl Default for GameSupportMatrix {
    fn default() -> Self {
        Self::create_default()
    }
}