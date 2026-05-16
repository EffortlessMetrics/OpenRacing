//! Moza command constants kept separate from command orchestration.

pub(super) const DIRECT_TORQUE_REPORT_ID: &str = "0x20";
pub(super) const SIMULATOR_FFB_WRITER_COMMAND: &str = "wheeld --hardware-lane moza-r5";
pub(super) const SIMULATOR_TELEMETRY_RECORDER_COMMAND: &str = "wheelctl telemetry record";
pub(super) const MOZA_VENDOR_HEX: &str = "0x346E";
pub(super) const HIGH_TORQUE_FEATURE_REPORT_ID: &str = "0x02";
pub(super) const START_REPORTING_FEATURE_REPORT_ID: &str = "0x03";
pub(super) const FFB_MODE_FEATURE_REPORT_ID: &str = "0x11";
pub(super) const PIDFF_SET_EFFECT_REPORT_ID: &str = "0x01";
pub(super) const PIDFF_SET_EFFECT_REPORT_ID_BYTE: u8 = 0x01;
pub(super) const PIDFF_SET_EFFECT_GENERIC_REPORT_LEN: usize = 14;
pub(super) const PIDFF_SET_EFFECT_R5_V1_REPORT_LEN: usize = 22;
pub(super) const PIDFF_EFFECT_TYPE_CONSTANT_FORCE: u8 = 0x01;
pub(super) const PIDFF_TRIGGER_BUTTON_NONE: u8 = 0xFF;
pub(super) const PIDFF_SET_CONSTANT_FORCE_REPORT_ID: &str = "0x05";
pub(super) const PIDFF_SET_CONSTANT_FORCE_REPORT_LEN: usize = 4;
pub(super) const PIDFF_EFFECT_OPERATION_REPORT_ID: &str = "0x0A";
pub(super) const PIDFF_EFFECT_OPERATION_REPORT_LEN: usize = 4;
pub(super) const PIDFF_BLOCK_FREE_REPORT_ID: &str = "0x0B";
pub(super) const PIDFF_BLOCK_FREE_REPORT_LEN: usize = 2;
pub(super) const PIDFF_CREATE_NEW_EFFECT_FEATURE_REPORT_ID: &str = "0x11";
pub(super) const PIDFF_BLOCK_LOAD_FEATURE_REPORT_ID: &str = "0x12";
pub(super) const PIDFF_PID_POOL_FEATURE_REPORT_ID: &str = "0x13";
pub(super) const PIDFF_DEVICE_CONTROL_REPORT_ID: &str = "0x0C";
pub(super) const PIDFF_DEVICE_CONTROL_REPORT_LEN: usize = 2;
pub(super) const PIDFF_STOP_ALL_EFFECTS_COMMAND: u8 = 0x04;
pub(super) const PIDFF_STOP_ALL_EFFECTS_COMMAND_NAME: &str = "stop_all_effects";
pub(super) const PIDFF_LOW_TORQUE_EFFECT_BLOCK_INDEX: u8 = 1;
pub(super) const PIDFF_LOW_TORQUE_LOOP_COUNT: u8 = 1;
pub(super) const PIDFF_LOW_TORQUE_DIRECTION_X: u16 = 9000;
pub(super) const PIDFF_LOW_TORQUE_DIRECTION_Y: u16 = 0;
pub(super) const PIDFF_EFFECT_SETUP_CLASSIFICATION: &str = "pidff_effect_setup";
pub(super) const PIDFF_BOUNDED_EFFECT_CLASSIFICATION: &str = "bounded_low_torque_pidff";
pub(super) const PIDFF_STOP_ALL_CLEANUP_CLASSIFICATION: &str = "pidff_stop_all_cleanup";
pub(super) const PIDFF_PLANNED_STOP_ALL_CLASSIFICATION: &str = "planned_pidff_stop_all";
pub(super) const R5_V1_LIVE_OUTPUT_REPORTS: &[(&str, usize)] = &[
    ("0x01", 22),
    ("0x02", 14),
    ("0x03", 15),
    ("0x04", 12),
    ("0x05", 4),
    ("0x06", 6),
    ("0x0A", 4),
    ("0x0B", 2),
    ("0x0C", 2),
    ("0x0D", 2),
    ("0x14", 51),
    ("0x15", 21),
    ("0xAF", 18),
];
pub(super) const R5_V1_LIVE_FEATURE_REPORT_IDS: &[&str] = &["0x11", "0x12", "0x13", "0xAF"];
pub(super) const MOZA_R5_MANIFEST_SCHEMA_JSON: &str =
    include_str!("../../../../../ci/hardware/moza-r5/manifest.schema.json");
pub(super) const SIMULATOR_FFB_PREREQUISITE_ARTIFACTS: [(&str, &str); 6] = [
    ("zero_torque_real_hardware", "zero-torque-proof.json"),
    ("watchdog_zero_output", "watchdog-proof.json"),
    ("disconnect_final_zero", "disconnect-proof.json"),
    ("init_off_handshake", "init-off.json"),
    ("init_standard_handshake", "init-standard.json"),
    ("low_torque_bounded", "low-torque-proof.json"),
];
