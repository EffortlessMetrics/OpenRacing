//! Game Support Matrix implementation

use crate::game_service::*;
use std::collections::HashMap;

impl GameSupportMatrix {
    /// Create default support matrix with iRacing, ACC, AMS2, rFactor 2 and EA WRC
    pub fn create_default() -> Self {
        let mut games = HashMap::new();

        // iRacing support
        games.insert(
            "iracing".to_string(),
            GameSupport {
                name: "iRacing".to_string(),
                versions: vec![GameVersion {
                    version: "2024.x".to_string(),
                    config_paths: vec!["Documents/iRacing/app.ini".to_string()],
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
                }],
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
                    install_paths: vec!["Program Files (x86)/iRacing".to_string()],
                },
            },
        );

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

        // Assetto Corsa Rally support (discovery-first probe profile)
        games.insert(
            "ac_rally".to_string(),
            GameSupport {
                name: "Assetto Corsa Rally".to_string(),
                versions: vec![GameVersion {
                    version: "Early Access".to_string(),
                    config_paths: vec![
                        "Documents/Assetto Corsa Rally/Config/openracing_probe.json".to_string(),
                    ],
                    executable_patterns: vec![],
                    telemetry_method: "probe_discovery".to_string(),
                    supported_fields: vec![],
                }],
                telemetry: TelemetrySupport {
                    method: "probe_discovery".to_string(),
                    update_rate_hz: 60,
                    fields: TelemetryFieldMapping {
                        ffb_scalar: None,
                        rpm: None,
                        speed_ms: None,
                        slip_ratio: None,
                        gear: None,
                        flags: None,
                        car_id: None,
                        track_id: None,
                    },
                },
                config_writer: "ac_rally".to_string(),
                auto_detect: AutoDetectConfig {
                    process_names: vec![],
                    install_registry_keys: vec![],
                    install_paths: vec![
                        "Program Files (x86)/Steam/steamapps/common/Assetto Corsa Rally"
                            .to_string(),
                    ],
                },
            },
        );

        // AMS2 support
        games.insert(
            "ams2".to_string(),
            GameSupport {
                name: "Automobilista 2".to_string(),
                versions: vec![GameVersion {
                    version: "1.5.x".to_string(),
                    config_paths: vec!["Documents/Automobilista 2/UserData/player/player.json".to_string()],
                    executable_patterns: vec!["AMS2AVX.exe".to_string()],
                    telemetry_method: "shared_memory".to_string(),
                    supported_fields: vec![
                        "ffb_scalar".to_string(),
                        "rpm".to_string(),
                        "speed_ms".to_string(),
                        "gear".to_string(),
                    ],
                }],
                telemetry: TelemetrySupport {
                    method: "shared_memory".to_string(),
                    update_rate_hz: 60,
                    fields: TelemetryFieldMapping {
                        ffb_scalar: Some("mSteering".to_string()),
                        rpm: Some("mRpm".to_string()),
                        speed_ms: Some("mSpeed".to_string()),
                        slip_ratio: None,
                        gear: Some("mGear".to_string()),
                        flags: None,
                        car_id: None,
                        track_id: None,
                    },
                },
                config_writer: "ams2".to_string(),
                auto_detect: AutoDetectConfig {
                    process_names: vec!["AMS2AVX.exe".to_string()],
                    install_registry_keys: vec![
                        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Steam App 1066890".to_string(),
                    ],
                    install_paths: vec![
                        "Program Files (x86)/Steam/steamapps/common/Automobilista 2".to_string(),
                    ],
                },
            },
        );

        // rFactor 2 support
        games.insert(
            "rfactor2".to_string(),
            GameSupport {
                name: "rFactor 2".to_string(),
                versions: vec![GameVersion {
                    version: "1.1.x".to_string(),
                    config_paths: vec!["UserData/player/OpenRacing.Telemetry.json".to_string()],
                    executable_patterns: vec![
                        "rFactor2.exe".to_string(),
                        "rFactor2 Dedicated.exe".to_string(),
                    ],
                    telemetry_method: "shared_memory".to_string(),
                    supported_fields: vec![
                        "ffb_scalar".to_string(),
                        "rpm".to_string(),
                        "speed_ms".to_string(),
                        "slip_ratio".to_string(),
                        "gear".to_string(),
                        "flags".to_string(),
                        "car_id".to_string(),
                        "track_id".to_string(),
                    ],
                }],
                telemetry: TelemetrySupport {
                    method: "shared_memory".to_string(),
                    update_rate_hz: 60,
                    fields: TelemetryFieldMapping {
                        ffb_scalar: Some("mForceFeedback".to_string()),
                        rpm: Some("mEngineRPM".to_string()),
                        speed_ms: Some("mLocalVel".to_string()),
                        slip_ratio: Some("mWheels[].mLateralPatchSlip".to_string()),
                        gear: Some("mGear".to_string()),
                        flags: Some("mGamePhase/mYellowFlagState/mInPits".to_string()),
                        car_id: Some("mVehicleName".to_string()),
                        track_id: Some("mTrackName".to_string()),
                    },
                },
                config_writer: "rfactor2".to_string(),
                auto_detect: AutoDetectConfig {
                    process_names: vec![
                        "rFactor2.exe".to_string(),
                        "rFactor2 Dedicated.exe".to_string(),
                    ],
                    install_registry_keys: vec![
                        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Steam App 365960".to_string(),
                    ],
                    install_paths: vec![
                        "Program Files (x86)/Steam/steamapps/common/rFactor 2".to_string(),
                    ],
                },
            },
        );

        // EA SPORTS WRC support
        games.insert(
            "eawrc".to_string(),
            GameSupport {
                name: "EA SPORTS WRC".to_string(),
                versions: vec![GameVersion {
                    version: "1.x".to_string(),
                    config_paths: vec!["Documents/My Games/WRC/telemetry/config.json".to_string()],
                    executable_patterns: vec![
                        "WRC.exe".to_string(),
                        "EASPORTSWRC.exe".to_string(),
                    ],
                    telemetry_method: "udp_schema".to_string(),
                    supported_fields: vec![
                        "ffb_scalar".to_string(),
                        "rpm".to_string(),
                        "speed_ms".to_string(),
                        "slip_ratio".to_string(),
                        "gear".to_string(),
                        "car_id".to_string(),
                        "track_id".to_string(),
                    ],
                }],
                telemetry: TelemetrySupport {
                    method: "udp_schema".to_string(),
                    update_rate_hz: 120,
                    fields: TelemetryFieldMapping {
                        ffb_scalar: Some("ffb_scalar".to_string()),
                        rpm: Some("engine_rpm".to_string()),
                        speed_ms: Some("vehicle_speed".to_string()),
                        slip_ratio: Some("slip_ratio".to_string()),
                        gear: Some("gear".to_string()),
                        flags: None,
                        car_id: Some("vehicle_id".to_string()),
                        track_id: Some("track_name".to_string()),
                    },
                },
                config_writer: "eawrc".to_string(),
                auto_detect: AutoDetectConfig {
                    process_names: vec!["WRC.exe".to_string(), "EASPORTSWRC.exe".to_string()],
                    install_registry_keys: vec![
                        "HKEY_LOCAL_MACHINE\\SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\Steam App 1849250".to_string(),
                    ],
                    install_paths: vec![
                        "Program Files (x86)/Steam/steamapps/common/WRC".to_string(),
                    ],
                },
            },
        );

        Self { games }
    }
}

impl Default for GameSupportMatrix {
    fn default() -> Self {
        Self::create_default()
    }
}
