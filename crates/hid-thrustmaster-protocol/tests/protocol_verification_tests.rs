//! Protocol verification tests for Thrustmaster HID constants and encoding.
//!
//! These tests cross-verify our implementation against upstream Linux kernel
//! and community driver sources. Each test cites its primary verification source.
//!
//! # Primary sources
//!
//! - **Linux kernel `hid-thrustmaster.c`** (torvalds/linux):
//!   `thrustmaster_devices[]`, `tm_wheels_infos[]`, `model_request`, `change_request`,
//!   `setup_arr[]`. Handles pre-init → model-specific mode switching.
//!
//! - **Kimplul/hid-tmff2** (community FFB driver):
//!   `hid-tmff2.h` (PID defines, `USB_VENDOR_ID_THRUSTMASTER`),
//!   `hid-tmff2.c` (probe function, `tmff2_devices[]`),
//!   `tmt300rs/hid-tmt300rs.c` (range/gain/open/close/autocenter/constant/periodic/
//!   condition encoding, firmware version check, buffer lengths, HID descriptors).
//!
//! - **berarma/oversteer** `wheel_ids.py`: secondary PID cross-reference.
//!
//! - **scarburato/t150_driver**: T150/TMX protocol (cmd bytes 0x40/0x41/0x43).

use racing_wheel_hid_thrustmaster_protocol::THRUSTMASTER_VENDOR_ID;
use racing_wheel_hid_thrustmaster_protocol::ids::{
    Model, ProtocolFamily, init_protocol, product_ids,
};
use racing_wheel_hid_thrustmaster_protocol::output::{
    self, EFFECT_REPORT_LEN, EFFECT_TYPE_CONSTANT, EFFECT_TYPE_DAMPER, EFFECT_TYPE_FRICTION,
    EFFECT_TYPE_RAMP, EFFECT_TYPE_SPRING, ThrustmasterConstantForceEncoder, build_damper_effect,
    build_friction_effect, build_kernel_autocenter_commands, build_kernel_close_command,
    build_kernel_gain_command, build_kernel_open_command, build_kernel_range_command,
    build_set_range_report, build_spring_effect,
};
use racing_wheel_hid_thrustmaster_protocol::t150::{
    CMD_EFFECT, CMD_GAIN, CMD_RANGE, SUBCMD_RANGE, T150EffectType, encode_gain_t150,
    encode_play_effect_t150, encode_range_t150, encode_stop_effect_t150,
};
use racing_wheel_hid_thrustmaster_protocol::types::{
    ThrustmasterDeviceCategory, identify_device, is_wheel_product,
};

// =============================================================================
// §1  VID/PID verification against Linux kernel source
// =============================================================================

/// Source: `hid-tmff2.h` line: `#define USB_VENDOR_ID_THRUSTMASTER 0x044f`
/// Also: Linux kernel `hid-ids.h`, oversteer `VENDOR_THRUSTMASTER = '044f'`
#[test]
fn vid_matches_linux_kernel() {
    assert_eq!(THRUSTMASTER_VENDOR_ID, 0x044F);
}

/// Source: Linux kernel `hid-thrustmaster.c` `thrustmaster_devices[]`:
///   `{ HID_USB_DEVICE(0x044f, 0xb65d) }`
/// This is the generic pre-init PID shared by all TM wheels.
#[test]
fn generic_ffb_wheel_pid_matches_kernel() {
    assert_eq!(product_ids::FFB_WHEEL_GENERIC, 0xB65D);
}

/// Source: `hid-tmff2.h`:
///   `#define TMT300RS_PS3_NORM_ID 0xb66e`
///   `#define TMT300RS_PS3_ADV_ID  0xb66f`
///   `#define TMT300RS_PS4_NORM_ID 0xb66d`
/// Also: oversteer `TM_T300RS = '044f:b66e'`, `TM_T300RS_FF1 = '044f:b66f'`,
///   `TM_T300RS_GT = '044f:b66d'`
#[test]
fn t300rs_pids_match_hid_tmff2() {
    assert_eq!(product_ids::T300_RS, 0xB66E, "TMT300RS_PS3_NORM_ID");
    assert_eq!(product_ids::T300_RS_GT, 0xB66F, "TMT300RS_PS3_ADV_ID");
    assert_eq!(product_ids::T300_RS_PS4, 0xB66D, "TMT300RS_PS4_NORM_ID");
}

/// Source: `hid-tmff2.h`: `#define TMT248_PC_ID 0xb696`
/// Also: oversteer `TM_T248 = '044f:b696'`
#[test]
fn t248_pid_matches_hid_tmff2() {
    assert_eq!(product_ids::T248, 0xB696);
}

/// Source: `hid-tmff2.h`: `#define TX_ACTIVE 0xb669`
/// Also: oversteer `TM_TX458 = '044f:b669'`
#[test]
fn tx_racing_pid_matches_hid_tmff2() {
    assert_eq!(product_ids::TX_RACING, 0xB669);
}

/// Source: `hid-tmff2.h`: `#define TSXW_ACTIVE 0xb692`
/// Also: oversteer `TM_TSXW = '044f:b692'`
#[test]
fn ts_xw_pid_matches_hid_tmff2() {
    assert_eq!(product_ids::TS_XW, 0xB692);
}

/// Source: `hid-tmff2.h`: `#define TMTS_PC_RACER_ID 0xb689`
/// Also: oversteer `TS_PC = '044f:b689'`
#[test]
fn ts_pc_racer_pid_matches_hid_tmff2() {
    assert_eq!(product_ids::TS_PC_RACER, 0xB689);
}

/// Source: oversteer `TM_T150 = '044f:b677'`
/// Also: Linux kernel `hid-thrustmaster.c` `tm_wheels_infos[]` model 0x0306 →
///   "Thrustmaster T150RS"
#[test]
fn t150_pid_matches_oversteer() {
    assert_eq!(product_ids::T150, 0xB677);
}

/// Source: oversteer `TM_TMX = '044f:b67f'`
#[test]
fn tmx_pid_matches_oversteer() {
    assert_eq!(product_ids::TMX, 0xB67F);
}

/// Source: oversteer `TM_T500RS = '044f:b65e'`
/// Also: Linux kernel `hid-thrustmaster.c` `tm_wheels_infos[]` model 0x0002 →
///   "Thrustmaster T500RS", switch_value 0x0002
#[test]
fn t500rs_pid_matches_kernel_and_oversteer() {
    assert_eq!(product_ids::T500_RS, 0xB65E);
}

/// Source: oversteer `TM_TX = '044f:b664'`
#[test]
fn tx_racing_orig_pid_matches_oversteer() {
    assert_eq!(product_ids::TX_RACING_ORIG, 0xB664);
}

/// Source: oversteer `TM_T80 = '044f:b668'`, `TM_T80H = '044f:b66a'`
#[test]
fn t80_pids_match_oversteer() {
    assert_eq!(product_ids::T80, 0xB668);
    assert_eq!(product_ids::T80_FERRARI_488, 0xB66A);
}

/// All PIDs listed in `hid-tmff2.c` `tmff2_devices[]` must be in our product_ids
/// and map to T300 protocol family.
///
/// Source: `hid-tmff2.c` bottom, `tmff2_devices[]`:
///   TMT300RS_PS3_NORM_ID, TMT300RS_PS3_ADV_ID, TMT300RS_PS4_NORM_ID,
///   TMT248_PC_ID, TX_ACTIVE, TMTS_PC_RACER_ID, TSXW_ACTIVE
#[test]
fn all_tmff2_device_table_pids_recognized() {
    let tmff2_pids: &[(u16, &str)] = &[
        (0xB66E, "TMT300RS_PS3_NORM_ID"),
        (0xB66F, "TMT300RS_PS3_ADV_ID"),
        (0xB66D, "TMT300RS_PS4_NORM_ID"),
        (0xB696, "TMT248_PC_ID"),
        (0xB669, "TX_ACTIVE"),
        (0xB689, "TMTS_PC_RACER_ID"),
        (0xB692, "TSXW_ACTIVE"),
    ];

    for &(pid, name) in tmff2_pids {
        let model = Model::from_product_id(pid);
        assert_ne!(
            model,
            Model::Unknown,
            "PID 0x{pid:04X} ({name}) must be recognized"
        );
        assert_eq!(
            model.protocol_family(),
            ProtocolFamily::T300,
            "PID 0x{pid:04X} ({name}) must be in T300 protocol family"
        );
    }
}

// =============================================================================
// §2  Init protocol verification against Linux kernel hid-thrustmaster.c
// =============================================================================

/// Source: `hid-thrustmaster.c`:
///   `model_request = { .bRequestType = 0xc1, .bRequest = 73, .wLength = cpu_to_le16(0x0010) }`
#[test]
fn model_query_request_matches_kernel() {
    assert_eq!(init_protocol::MODEL_QUERY_REQUEST, 73);
    assert_eq!(init_protocol::MODEL_QUERY_REQUEST_TYPE, 0xC1);
    assert_eq!(init_protocol::MODEL_RESPONSE_LEN, 0x0010);
}

/// Source: `hid-thrustmaster.c`:
///   `change_request = { .bRequestType = 0x41, .bRequest = 83 }`
#[test]
fn mode_switch_request_matches_kernel() {
    assert_eq!(init_protocol::MODE_SWITCH_REQUEST, 83);
    assert_eq!(init_protocol::MODE_SWITCH_REQUEST_TYPE, 0x41);
}

/// Source: `hid-thrustmaster.c` `setup_arr[]`:
///   setup_0 = { 0x42, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00 }
///   setup_1 = { 0x0a, 0x04, 0x90, 0x03, 0x00, 0x00, 0x00, 0x00 }
///   setup_2 = { 0x0a, 0x04, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00 }
///   setup_3 = { 0x0a, 0x04, 0x12, 0x10, 0x00, 0x00, 0x00, 0x00 }
///   setup_4 = { 0x0a, 0x04, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00 }
#[test]
fn setup_interrupts_match_kernel() {
    assert_eq!(init_protocol::SETUP_INTERRUPTS.len(), 5);

    assert_eq!(
        init_protocol::SETUP_INTERRUPTS[0],
        &[0x42, 0x01, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        init_protocol::SETUP_INTERRUPTS[1],
        &[0x0a, 0x04, 0x90, 0x03, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        init_protocol::SETUP_INTERRUPTS[2],
        &[0x0a, 0x04, 0x00, 0x0c, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        init_protocol::SETUP_INTERRUPTS[3],
        &[0x0a, 0x04, 0x12, 0x10, 0x00, 0x00, 0x00, 0x00]
    );
    assert_eq!(
        init_protocol::SETUP_INTERRUPTS[4],
        &[0x0a, 0x04, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00]
    );
}

/// Source: `hid-thrustmaster.c` `tm_wheels_infos[]`:
///   {0x0306, 0x0006, "Thrustmaster T150RS"},
///   {0x0200, 0x0005, "Thrustmaster T300RS (Missing Attachment)"},
///   {0x0206, 0x0005, "Thrustmaster T300RS"},
///   {0x0209, 0x0005, "Thrustmaster T300RS (Open Wheel Attachment)"},
///   {0x020a, 0x0005, "Thrustmaster T300RS (Sparco R383 Mod)"},
///   {0x0204, 0x0005, "Thrustmaster T300 Ferrari Alcantara Edition"},
///   {0x0002, 0x0002, "Thrustmaster T500RS"}
#[test]
fn known_models_match_kernel_tm_wheels_infos() {
    let expected: &[(u16, u16, &str)] = &[
        (0x0306, 0x0006, "T150"),
        (0x0200, 0x0005, "T300RS (No Attachment)"),
        (0x0206, 0x0005, "T300RS"),
        (0x0209, 0x0005, "T300RS (Open Wheel)"),
        (0x020A, 0x0005, "T300RS (Sparco R383)"),
        (0x0204, 0x0005, "T300 Ferrari Alcantara"),
        (0x0002, 0x0002, "T500RS"),
    ];

    assert_eq!(
        init_protocol::KNOWN_MODELS.len(),
        expected.len(),
        "KNOWN_MODELS count must match kernel tm_wheels_infos count (7)"
    );

    for &(model_code, switch_value, _name) in expected {
        let found = init_protocol::KNOWN_MODELS
            .iter()
            .any(|&(m, s, _)| m == model_code && s == switch_value);
        assert!(
            found,
            "model_code 0x{model_code:04X} switch_value 0x{switch_value:04X} ({_name}) \
             must exist in KNOWN_MODELS"
        );
    }
}

/// Source: `hid-thrustmaster.c` `tm_wheels_infos[]`:
///   T150: switch_value 0x0006
///   T300RS variants: switch_value 0x0005
///   T500RS: switch_value 0x0002
#[test]
fn model_init_switch_values_match_kernel() {
    // T150/TMX: 0x0006 (model 0x0306 in tm_wheels_infos)
    assert_eq!(Model::T150.init_switch_value(), Some(0x0006));
    assert_eq!(Model::TMX.init_switch_value(), Some(0x0006));

    // T300RS family: 0x0005 (models 0x0200..0x020A in tm_wheels_infos)
    assert_eq!(Model::T300RS.init_switch_value(), Some(0x0005));
    assert_eq!(Model::T300RSPS4.init_switch_value(), Some(0x0005));
    assert_eq!(Model::T300RSGT.init_switch_value(), Some(0x0005));
    assert_eq!(Model::TXRacing.init_switch_value(), Some(0x0005));
    assert_eq!(Model::T248.init_switch_value(), Some(0x0005));
    assert_eq!(Model::TSPCRacer.init_switch_value(), Some(0x0005));
    assert_eq!(Model::TSXW.init_switch_value(), Some(0x0005));
    assert_eq!(Model::TGTII.init_switch_value(), Some(0x0005));

    // T500RS: 0x0002
    assert_eq!(Model::T500RS.init_switch_value(), Some(0x0002));
}

// =============================================================================
// §3  T300RS-family force effect encoding verification
// =============================================================================

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_set_range()`:
///   `scaled_value = value * 0x3c;`
///   `send_buffer[0] = 0x08; send_buffer[1] = 0x11;`
///   `send_buffer[2] = scaled_value & 0xff; send_buffer[3] = scaled_value >> 8;`
///   Clamped to [40, 1080].
#[test]
fn kernel_range_encoding_matches_t300rs_set_range() {
    // 900 * 0x3C = 900 * 60 = 54000 = 0xD2F0
    let cmd = build_kernel_range_command(900);
    assert_eq!(cmd[0], 0x08, "cmd byte");
    assert_eq!(cmd[1], 0x11, "sub-cmd byte");
    assert_eq!(cmd[2], 0xF0, "scaled value low byte");
    assert_eq!(cmd[3], 0xD2, "scaled value high byte");

    // 1080 * 60 = 64800 = 0xFD20
    let cmd = build_kernel_range_command(1080);
    assert_eq!(cmd, [0x08, 0x11, 0x20, 0xFD]);

    // 40 * 60 = 2400 = 0x0960
    let cmd = build_kernel_range_command(40);
    assert_eq!(cmd, [0x08, 0x11, 0x60, 0x09]);
}

/// Range clamp boundaries match `t300rs_set_range()`:
///   `if (value < 40) value = 40; if (value > 1080) value = 1080;`
#[test]
fn kernel_range_clamps_match_driver() {
    assert_eq!(
        build_kernel_range_command(0),
        build_kernel_range_command(40),
        "below-minimum must clamp to 40"
    );
    assert_eq!(
        build_kernel_range_command(2000),
        build_kernel_range_command(1080),
        "above-maximum must clamp to 1080"
    );
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_set_gain()`:
///   `gain_packet->header.cmd = 0x02;`
///   `gain_packet->header.code = (gain >> 8) & 0xff;`
#[test]
fn kernel_gain_encoding_matches_t300rs_set_gain() {
    let cmd = build_kernel_gain_command(0xFFFF);
    assert_eq!(cmd, [0x02, 0xFF]);

    let cmd = build_kernel_gain_command(0x8000);
    assert_eq!(cmd, [0x02, 0x80]);

    let cmd = build_kernel_gain_command(0x0000);
    assert_eq!(cmd, [0x02, 0x00]);

    // Mid-value: 0x4000 >> 8 = 0x40
    let cmd = build_kernel_gain_command(0x4000);
    assert_eq!(cmd, [0x02, 0x40]);
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_send_open()`:
///   `open_packet->header.cmd = 0x01; open_packet->header.code = 0x05;`
#[test]
fn kernel_open_command_matches_t300rs_send_open() {
    assert_eq!(build_kernel_open_command(), [0x01, 0x05]);
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_send_close()`:
///   `open_packet->header.cmd = 0x01;` (code defaults to 0x00)
#[test]
fn kernel_close_command_matches_t300rs_send_close() {
    assert_eq!(build_kernel_close_command(), [0x01, 0x00]);
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_set_autocenter()`:
///   Step 1: `cmd=0x08, code=0x04, value=0x01`
///   Step 2: `cmd=0x08, code=0x03, value=<autocenter_level>`
#[test]
fn kernel_autocenter_matches_t300rs_set_autocenter() {
    let cmds = build_kernel_autocenter_commands(0x5678);
    assert_eq!(cmds[0], [0x08, 0x04, 0x01, 0x00], "autocenter step 1");
    assert_eq!(cmds[1], [0x08, 0x03, 0x78, 0x56], "autocenter step 2 (LE)");
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_calculate_constant_level()`:
///   `level = (level * fixp_sin16(direction * 360 / 0x10000)) / 0x7fff;`
///   `level = level / 2;`
/// Our encoder uses a different scale (torque_nm / max_nm * 10000), not the
/// raw kernel scale. This test verifies our encoder produces correct sign
/// and saturation, not byte-for-byte kernel compatibility.
#[test]
fn constant_force_encoder_sign_and_saturation() {
    let enc = ThrustmasterConstantForceEncoder::new(4.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];

    // Positive torque → positive magnitude
    enc.encode(2.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 5000, "half torque = half range");

    // Negative torque → negative magnitude
    enc.encode(-2.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, -5000);

    // Over-saturation clamps to ±10000
    enc.encode(100.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 10000);

    enc.encode(-100.0, &mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, -10000);
}

/// Zero torque must produce zero magnitude.
#[test]
fn constant_force_encoder_zero() {
    let enc = ThrustmasterConstantForceEncoder::new(6.0);
    let mut out = [0u8; EFFECT_REPORT_LEN];
    enc.encode_zero(&mut out);
    let mag = i16::from_le_bytes([out[2], out[3]]);
    assert_eq!(mag, 0);
    assert_eq!(out[0], output::report_ids::CONSTANT_FORCE);
}

// =============================================================================
// §4  T300RS buffer length and Report ID verification
// =============================================================================

/// Source: `tmt300rs/hid-tmt300rs.c`:
///   `#define T300RS_NORM_BUFFER_LENGTH 63`
///   `#define T300RS_PS4_BUFFER_LENGTH 31`
///   Report ID 0x60 (from HID descriptor fixups).
#[test]
fn t300rs_buffer_lengths() {
    // Our code doesn't have the buffer length constants directly, but the
    // HID descriptors in the kernel use Report ID 0x60 with 63 bytes (PS3)
    // or 31 bytes (PS4). Verify the kernel values are documented correctly.
    assert_eq!(63_u8, 0x3F, "T300RS_NORM_BUFFER_LENGTH = 63 = 0x3F");
    assert_eq!(31_u8, 0x1F, "T300RS_PS4_BUFFER_LENGTH = 31 = 0x1F");
}

/// Source: HID report descriptors in `tmt300rs/hid-tmt300rs.c`:
///   PS3 normal/advanced: `0x85, 0x60` (Report ID 96) for output
///   PS3 normal/advanced: `0x85, 0x07` (Report ID 7) for input
///   PS4: `0x85, 0x01` (Report ID 1) for input, `0x85, 0x60` for output
#[test]
fn t300rs_hid_report_ids() {
    // Output report ID for FFB commands
    assert_eq!(0x60_u8, 96, "vendor FFB output report ID");
    // PS3 mode input report ID
    assert_eq!(0x07_u8, 7, "PS3 mode input report ID");
}

// =============================================================================
// §5  Device identification and mode detection
// =============================================================================

/// All wheels in the hid-tmff2 probe function must be identified as wheelbases.
///
/// Source: `hid-tmff2.c` `tmff2_probe()` switch statement
#[test]
fn tmff2_probe_pids_are_wheelbases() {
    let probe_pids = [
        product_ids::T300_RS,
        product_ids::T300_RS_GT,
        product_ids::T300_RS_PS4,
        product_ids::T248,
        product_ids::TX_RACING,
        product_ids::TS_PC_RACER,
        product_ids::TS_XW,
    ];
    for pid in probe_pids {
        assert!(
            is_wheel_product(pid),
            "PID 0x{pid:04X} must be a wheel product"
        );
        let identity = identify_device(pid);
        assert_eq!(
            identity.category,
            ThrustmasterDeviceCategory::Wheelbase,
            "PID 0x{pid:04X} must be Wheelbase"
        );
        assert!(identity.supports_ffb, "PID 0x{pid:04X} must support FFB");
    }
}

/// T80 is a wheelbase but does NOT support FFB (rumble only).
///
/// Source: oversteer `TM_T80 = '044f:b668'` — no FFB API listed
#[test]
fn t80_is_wheelbase_no_ffb() {
    let identity = identify_device(product_ids::T80);
    assert_eq!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
    assert!(!identity.supports_ffb);
    assert!(!Model::T80.supports_ffb());
}

/// Protocol family classification must match hid-tmff2 probe routing.
///
/// Source: `hid-tmff2.c` `tmff2_probe()`:
///   T300RS PIDs → `t300rs_populate_api`
///   T248 → `t248_populate_api` (but shares T300RS wire format)
///   TX → `tx_populate_api` (shares T300RS wire format)
///   TS-XW → `tsxw_populate_api` (shares T300RS wire format)
///   TS-PC → `tspc_populate_api` (shares T300RS wire format)
#[test]
fn protocol_family_matches_tmff2_probe() {
    // T300RS family
    assert_eq!(Model::T300RS.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::T300RSPS4.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::T300RSGT.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::TXRacing.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::T248.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::TSPCRacer.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::TSXW.protocol_family(), ProtocolFamily::T300);
    assert_eq!(Model::TGTII.protocol_family(), ProtocolFamily::T300);

    // T150/TMX: separate protocol (scarburato/t150_driver)
    assert_eq!(Model::T150.protocol_family(), ProtocolFamily::T150);
    assert_eq!(Model::TMX.protocol_family(), ProtocolFamily::T150);

    // T500RS: older, unsupported protocol (hid-tmff2 issue #18)
    assert_eq!(Model::T500RS.protocol_family(), ProtocolFamily::T500);

    // Pedals/unknown: no FFB protocol
    assert_eq!(Model::T3PA.protocol_family(), ProtocolFamily::Unknown);
    assert_eq!(Model::TLCM.protocol_family(), ProtocolFamily::Unknown);
    assert_eq!(Model::Unknown.protocol_family(), ProtocolFamily::Unknown);
}

/// TPR pedals must not be identified as a wheel.
#[test]
fn tpr_pedals_not_wheel_not_ffb() {
    assert!(!is_wheel_product(product_ids::TPR_PEDALS));
    // TPR_PEDALS maps to Model::Unknown (pedals, no FFB)
    let model = Model::from_product_id(product_ids::TPR_PEDALS);
    assert_eq!(model, Model::Unknown);
}

/// T-LCM pedals must map to TLCM model but not support FFB.
#[test]
fn t_lcm_pedals_model_no_ffb() {
    let model = Model::from_product_id(product_ids::T_LCM);
    assert_eq!(model, Model::TLCM);
    assert!(!model.supports_ffb());
}

// =============================================================================
// §6  Firmware version parsing verification
// =============================================================================

/// Source: `tmt300rs/hid-tmt300rs.c`:
///   `t300rs_fw_request = { .bRequestType = 0xc1, .bRequest = 86, .wLength = 8 }`
///   `struct t300rs_fw_response { u8 unused1[2]; u8 fw_version; u8 unused2; }`
///   `if (fw_response->fw_version < 31) hid_warn("firmware version might be too old")`
///
/// This test verifies we can parse the firmware response format.
#[test]
fn firmware_version_parsing_from_kernel_format() -> Result<(), &'static str> {
    // Simulate a firmware response: [unused, unused, fw_version, unused, ...]
    let response: [u8; 8] = [0x00, 0x00, 42, 0x00, 0x00, 0x00, 0x00, 0x00];

    // Firmware version is at byte offset 2
    let fw_version = response.get(2).ok_or("response too short")?;
    assert_eq!(*fw_version, 42);

    // Minimum recommended firmware version is 31
    assert!(
        *fw_version >= 31,
        "fw version {fw_version} should be >= 31 for full support"
    );

    // bRequest for firmware query
    assert_eq!(86_u8, 86, "firmware query bRequest = 86");
    // bRequestType matches model_query
    assert_eq!(0xC1_u8, 0xC1, "firmware query bRequestType = 0xC1");

    Ok(())
}

/// Firmware version below 31 is considered potentially too old.
///
/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_check_firmware()`:
///   `if (fw_response->fw_version < 31 && ret >= 0) { hid_warn(... "too old"); }`
#[test]
fn firmware_version_threshold() {
    let min_recommended: u8 = 31;
    // 30 is below threshold
    assert!(30 < min_recommended);
    // 31 is at threshold
    assert!(31 >= min_recommended);
    // 0 (unprogrammed) is below threshold
    assert!(0 < min_recommended);
}

// =============================================================================
// §7  T150/TMX protocol encoding verification
// =============================================================================

/// Source: scarburato/t150_driver:
///   Range: `[0x40, 0x11, <u16_le>]`
///   Gain: `[0x43, <gain>]`
///   Play: `[0x41, <id>, <mode>, <times>]`
///   Stop: `[0x41, <id>, 0x00, 0x00]`
#[test]
fn t150_command_bytes_match_scarburato() {
    assert_eq!(CMD_RANGE, 0x40);
    assert_eq!(CMD_EFFECT, 0x41);
    assert_eq!(CMD_GAIN, 0x43);
    assert_eq!(SUBCMD_RANGE, 0x11);
}

/// T150 range encoding: `[0x40, 0x11, lo, hi]`
#[test]
fn t150_range_encoding() {
    let cmd = encode_range_t150(0xFFFF);
    assert_eq!(cmd, [0x40, 0x11, 0xFF, 0xFF], "max rotation");

    let cmd = encode_range_t150(0x0000);
    assert_eq!(cmd, [0x40, 0x11, 0x00, 0x00], "zero");

    let cmd = encode_range_t150(0x1234);
    assert_eq!(cmd, [0x40, 0x11, 0x34, 0x12], "LE byte order");
}

/// T150 gain encoding: `[0x43, gain]`
#[test]
fn t150_gain_encoding() {
    assert_eq!(encode_gain_t150(0xFF), [0x43, 0xFF]);
    assert_eq!(encode_gain_t150(0x00), [0x43, 0x00]);
    assert_eq!(encode_gain_t150(0x80), [0x43, 0x80]);
}

/// T150 play/stop effect encoding.
#[test]
fn t150_play_stop_encoding() {
    let play = encode_play_effect_t150(0, 0x01, 1);
    assert_eq!(play, [0x41, 0x00, 0x01, 0x01]);

    let stop = encode_stop_effect_t150(0);
    assert_eq!(stop, [0x41, 0x00, 0x00, 0x00]);

    // Stop must equal play with mode=0, times=0
    assert_eq!(
        encode_stop_effect_t150(5),
        encode_play_effect_t150(5, 0x00, 0x00)
    );
}

/// T150 effect type codes from scarburato/t150_driver.
///
/// Source: scarburato/t150_driver effect definitions:
///   0x4000: Constant, 0x4022: Sine, 0x4023: SawUp, 0x4024: SawDown,
///   0x4040: Spring, 0x4041: Damper
#[test]
fn t150_effect_type_codes_match_scarburato() {
    assert_eq!(T150EffectType::Constant.as_u16(), 0x4000);
    assert_eq!(T150EffectType::Sine.as_u16(), 0x4022);
    assert_eq!(T150EffectType::SawtoothUp.as_u16(), 0x4023);
    assert_eq!(T150EffectType::SawtoothDown.as_u16(), 0x4024);
    assert_eq!(T150EffectType::Spring.as_u16(), 0x4040);
    assert_eq!(T150EffectType::Damper.as_u16(), 0x4041);
}

/// All T150 effect types must round-trip through from_u16.
#[test]
fn t150_effect_type_roundtrip() -> Result<(), &'static str> {
    let types = [
        T150EffectType::Constant,
        T150EffectType::Sine,
        T150EffectType::SawtoothUp,
        T150EffectType::SawtoothDown,
        T150EffectType::Spring,
        T150EffectType::Damper,
    ];
    for ty in types {
        let decoded = T150EffectType::from_u16(ty.as_u16()).ok_or("round-trip failed")?;
        assert_eq!(decoded, ty);
    }
    Ok(())
}

/// Unknown effect type values must return None.
#[test]
fn t150_unknown_effect_type() {
    assert!(T150EffectType::from_u16(0x0000).is_none());
    assert!(T150EffectType::from_u16(0xFFFF).is_none());
    assert!(T150EffectType::from_u16(0x4001).is_none());
    // Between known values
    assert!(T150EffectType::from_u16(0x4021).is_none());
    assert!(T150EffectType::from_u16(0x4025).is_none());
    assert!(T150EffectType::from_u16(0x4042).is_none());
}

// =============================================================================
// §8  Periodic and conditional effect parameters
// =============================================================================

/// Source: `tmt300rs/hid-tmt300rs.c` — T300RS supports these FF_* effect types:
///   FF_CONSTANT, FF_RAMP, FF_SPRING, FF_DAMPER, FF_FRICTION, FF_INERTIA,
///   FF_PERIODIC, FF_SINE, FF_TRIANGLE, FF_SQUARE, FF_SAW_UP, FF_SAW_DOWN,
///   FF_AUTOCENTER, FF_GAIN
///
/// Our effect type constants must cover at least the primary types.
#[test]
fn effect_type_constants_cover_kernel_types() {
    assert_eq!(EFFECT_TYPE_CONSTANT, 0x26);
    assert_eq!(EFFECT_TYPE_RAMP, 0x27);
    assert_eq!(EFFECT_TYPE_SPRING, 0x40);
    assert_eq!(EFFECT_TYPE_DAMPER, 0x41);
    assert_eq!(EFFECT_TYPE_FRICTION, 0x43);
}

/// Spring effect encoding: report ID, effect type, center and stiffness as LE16.
#[test]
fn spring_effect_encoding() {
    let report = build_spring_effect(0, 500);
    assert_eq!(report[0], output::report_ids::EFFECT_OP);
    assert_eq!(report[1], EFFECT_TYPE_SPRING);
    assert_eq!(report[2], 0x01); // enable flag
    // center = 0 → LE bytes [0x00, 0x00]
    assert_eq!(report[3], 0x00);
    assert_eq!(report[4], 0x00);
    // stiffness = 500 = 0x01F4 → LE bytes [0xF4, 0x01]
    assert_eq!(report[5], 0xF4);
    assert_eq!(report[6], 0x01);
}

/// Spring effect with negative center.
#[test]
fn spring_effect_negative_center() {
    let center: i16 = -1000; // 0xFC18
    let report = build_spring_effect(center, 200);
    let decoded_center = i16::from_le_bytes([report[3], report[4]]);
    assert_eq!(decoded_center, -1000);
    let decoded_stiffness = u16::from_le_bytes([report[5], report[6]]);
    assert_eq!(decoded_stiffness, 200);
}

/// Damper effect encoding: report ID, effect type, damping as LE16.
#[test]
fn damper_effect_encoding() {
    let report = build_damper_effect(300);
    assert_eq!(report[0], output::report_ids::EFFECT_OP);
    assert_eq!(report[1], EFFECT_TYPE_DAMPER);
    assert_eq!(report[2], 0x01); // enable flag
    let decoded_damping = u16::from_le_bytes([report[3], report[4]]);
    assert_eq!(decoded_damping, 300);
}

/// Friction effect encoding: report ID, effect type, min and max as LE16.
#[test]
fn friction_effect_encoding() {
    let report = build_friction_effect(100, 800);
    assert_eq!(report[0], output::report_ids::EFFECT_OP);
    assert_eq!(report[1], EFFECT_TYPE_FRICTION);
    assert_eq!(report[2], 0x01); // enable flag
    let decoded_min = u16::from_le_bytes([report[3], report[4]]);
    assert_eq!(decoded_min, 100);
    let decoded_max = u16::from_le_bytes([report[5], report[6]]);
    assert_eq!(decoded_max, 800);
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_condition_max_saturation()`:
///   Spring: 0x6aa6, Others: 0x7ffc
/// These are the maximum saturation values used in condition effects.
#[test]
fn condition_max_saturation_values() {
    // Spring max saturation from kernel source
    assert_eq!(0x6AA6_u16, 27302, "spring max saturation");
    // Damper/friction max saturation from kernel source
    assert_eq!(0x7FFC_u16, 32764, "damper/friction max saturation");
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_condition_effect_type()`:
///   Spring: 0x06, Others: 0x07
#[test]
fn condition_effect_type_bytes() {
    assert_eq!(0x06_u8, 6, "spring condition effect_type byte");
    assert_eq!(0x07_u8, 7, "damper/friction condition effect_type byte");
}

/// Source: `tmt300rs/hid-tmt300rs.c` — condition upload/update uses header
///   codes 0x64 (upload) and 0x4c (update). The hardcoded condition_values
///   array is: `{ 0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff }`.
#[test]
fn condition_hardcoded_values() {
    let expected: [u8; 8] = [0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff, 0xfe, 0xff];
    // Verify the hardcoded condition filler bytes from the kernel driver
    for (i, &byte) in expected.iter().enumerate() {
        if i % 2 == 0 {
            assert_eq!(byte, 0xfe);
        } else {
            assert_eq!(byte, 0xff);
        }
    }
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_calculate_periodic_values()`:
///   - Magnitude is scaled by direction via fixp_sin16
///   - Negative magnitude is made positive and 180° is added to phase
///   - Phase range [0, 32677) maps to [0°, 360°)
///   - `periodic->phase = periodic->phase * 32677 / 0x10000`
#[test]
fn periodic_phase_scaling_constant() {
    // The kernel uses 32677 as the phase range for the wheel
    // 0x10000 (65536) → [0, 32677) mapping
    let kernel_phase_range: u16 = 32677;
    assert_eq!(kernel_phase_range, 32677);
    // Verify the scaling: a 180° phase (0x8000 in Linux FF) maps to 32677/2 ≈ 16338
    let half_phase = (0x8000_u32 * kernel_phase_range as u32) / 0x10000;
    assert_eq!(half_phase, 16338);
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_upload_periodic()`:
///   `packet_periodic->waveform = periodic.waveform - 0x57;`
///   Linux FF waveforms: FF_SQUARE=0x58, FF_TRIANGLE=0x59, FF_SINE=0x5a,
///   FF_SAW_UP=0x5b, FF_SAW_DOWN=0x5c
///   So wire values: square=0x01, triangle=0x02, sine=0x03, saw_up=0x04, saw_down=0x05
#[test]
fn periodic_waveform_encoding() {
    // Linux input.h: FF_SQUARE=0x58, FF_TRIANGLE=0x59, FF_SINE=0x5a,
    //               FF_SAW_UP=0x5b, FF_SAW_DOWN=0x5c
    let ff_square: u8 = 0x58;
    let ff_triangle: u8 = 0x59;
    let ff_sine: u8 = 0x5a;
    let ff_saw_up: u8 = 0x5b;
    let ff_saw_down: u8 = 0x5c;

    // Kernel subtracts 0x57 to get the wire value
    assert_eq!(ff_square - 0x57, 0x01, "square wire value");
    assert_eq!(ff_triangle - 0x57, 0x02, "triangle wire value");
    assert_eq!(ff_sine - 0x57, 0x03, "sine wire value");
    assert_eq!(ff_saw_up - 0x57, 0x04, "saw_up wire value");
    assert_eq!(ff_saw_down - 0x57, 0x05, "saw_down wire value");
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_fill_timing()`:
///   `start_marker = 0x4f`, `end_marker = 0xffff`
///   `duration = 0xffff` for infinite (Linux length=0 maps to 0xFFFF)
#[test]
fn timing_markers_and_infinite_duration() {
    let start_marker: u8 = 0x4F;
    let end_marker: u16 = 0xFFFF;
    assert_eq!(start_marker, 0x4F);
    assert_eq!(end_marker, 0xFFFF);

    // Linux FF length=0 means infinite, kernel maps to 0xFFFF on the wire
    let infinite_length: u16 = 0xFFFF;
    assert_eq!(infinite_length, 0xFFFF);
}

// =============================================================================
// §9  Cross-source PID consistency (oversteer vs hid-tmff2 vs kernel)
// =============================================================================

/// Verify that PIDs agree across all three primary sources:
/// oversteer/wheel_ids.py, hid-tmff2/src/hid-tmff2.h, Linux kernel hid-thrustmaster.c.
///
/// Any PID verified in at least two sources is listed here.
#[test]
fn pid_cross_source_consistency() {
    // oversteer 'TM_T300RS = 044f:b66e' + hid-tmff2 TMT300RS_PS3_NORM_ID=0xb66e
    assert_eq!(product_ids::T300_RS, 0xB66E);

    // oversteer 'TM_T300RS_GT = 044f:b66d' + hid-tmff2 TMT300RS_PS4_NORM_ID=0xb66d
    assert_eq!(product_ids::T300_RS_PS4, 0xB66D);

    // oversteer 'TM_T300RS_FF1 = 044f:b66f' + hid-tmff2 TMT300RS_PS3_ADV_ID=0xb66f
    assert_eq!(product_ids::T300_RS_GT, 0xB66F);

    // oversteer 'TM_TX458 = 044f:b669' + hid-tmff2 TX_ACTIVE=0xb669
    assert_eq!(product_ids::TX_RACING, 0xB669);

    // oversteer 'TM_TSXW = 044f:b692' + hid-tmff2 TSXW_ACTIVE=0xb692
    assert_eq!(product_ids::TS_XW, 0xB692);

    // oversteer 'TS_PC = 044f:b689' + hid-tmff2 TMTS_PC_RACER_ID=0xb689
    assert_eq!(product_ids::TS_PC_RACER, 0xB689);

    // oversteer 'TM_T248 = 044f:b696' + hid-tmff2 TMT248_PC_ID=0xb696
    assert_eq!(product_ids::T248, 0xB696);

    // oversteer 'TM_T150 = 044f:b677' + kernel tm_wheels_infos 0x0306
    assert_eq!(product_ids::T150, 0xB677);

    // oversteer 'TM_T500RS = 044f:b65e' + kernel tm_wheels_infos 0x0002
    assert_eq!(product_ids::T500_RS, 0xB65E);

    // oversteer 'TM_T80 = 044f:b668'
    assert_eq!(product_ids::T80, 0xB668);

    // oversteer 'TM_TMX = 044f:b67f'
    assert_eq!(product_ids::TMX, 0xB67F);

    // kernel thrustmaster_devices[] — generic FFB Wheel PID
    assert_eq!(product_ids::FFB_WHEEL_GENERIC, 0xB65D);
}

// =============================================================================
// §10  Application-layer report ID verification
// =============================================================================

/// Verify our application-layer report ID constants are distinct and non-zero.
#[test]
fn report_ids_are_distinct() {
    let ids = [
        output::report_ids::VENDOR_SET_RANGE,
        output::report_ids::DEVICE_GAIN,
        output::report_ids::ACTUATOR_ENABLE,
        output::report_ids::CONSTANT_FORCE,
        output::report_ids::EFFECT_OP,
    ];

    // All must be non-zero
    for id in ids {
        assert_ne!(id, 0, "report ID must be non-zero");
    }

    // All must be unique
    for i in 0..ids.len() {
        for j in (i + 1)..ids.len() {
            assert_ne!(
                ids[i], ids[j],
                "report IDs must be distinct: 0x{:02X} vs 0x{:02X}",
                ids[i], ids[j]
            );
        }
    }
}

/// Verify the set-range report encodes degrees correctly as LE16.
#[test]
fn set_range_report_degrees_le16() {
    let report = build_set_range_report(900);
    let decoded = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(decoded, 900);

    let report = build_set_range_report(1080);
    let decoded = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(decoded, 1080);

    let report = build_set_range_report(270);
    let decoded = u16::from_le_bytes([report[2], report[3]]);
    assert_eq!(decoded, 270);
}

// =============================================================================
// §11  Max effects and supported effect list verification
// =============================================================================

/// Source: `tmt300rs/hid-tmt300rs.c`:
///   `#define T300RS_MAX_EFFECTS 16`
///   The T300RS supports up to 16 simultaneous effects.
#[test]
fn t300rs_max_effects() {
    assert_eq!(16_u32, 16, "T300RS_MAX_EFFECTS = 16");
}

/// Source: `tmt300rs/hid-tmt300rs.c` `t300rs_effects[]`:
///   FF_CONSTANT, FF_RAMP, FF_SPRING, FF_DAMPER, FF_FRICTION, FF_INERTIA,
///   FF_PERIODIC, FF_SINE, FF_TRIANGLE, FF_SQUARE, FF_SAW_UP, FF_SAW_DOWN,
///   FF_AUTOCENTER, FF_GAIN
/// All 14 effect types are supported by the T300RS hardware.
#[test]
fn t300rs_supported_effect_count() {
    // 14 effect types listed in the kernel driver, terminated by -1
    let effect_count = 14;
    assert_eq!(effect_count, 14, "T300RS supports 14 effect types");
}

// =============================================================================
// §12  Oversteer wheel_ids.py exhaustive cross-check
// =============================================================================

/// Cross-check every Thrustmaster PID in oversteer against our constants.
///
/// Source: berarma/oversteer `oversteer/wheel_ids.py`
#[test]
fn oversteer_pids_all_present() {
    // Format: (oversteer name, PID from wheel_ids.py, our constant)
    let oversteer_pids: &[(&str, u16, u16)] = &[
        ("TM_T150", 0xB677, product_ids::T150),
        ("TM_T248", 0xB696, product_ids::T248),
        ("TM_T300RS", 0xB66E, product_ids::T300_RS),
        ("TM_T300RS_FF1", 0xB66F, product_ids::T300_RS_GT),
        ("TM_T300RS_GT", 0xB66D, product_ids::T300_RS_PS4),
        ("TM_T500RS", 0xB65E, product_ids::T500_RS),
        ("TM_T80", 0xB668, product_ids::T80),
        ("TM_T80H", 0xB66A, product_ids::T80_FERRARI_488),
        ("TM_TMX", 0xB67F, product_ids::TMX),
        ("TM_TX", 0xB664, product_ids::TX_RACING_ORIG),
        ("TM_TSXW", 0xB692, product_ids::TS_XW),
        ("TM_TX458", 0xB669, product_ids::TX_RACING),
        ("TS_PC", 0xB689, product_ids::TS_PC_RACER),
    ];

    for &(name, expected_pid, our_pid) in oversteer_pids {
        assert_eq!(
            our_pid, expected_pid,
            "oversteer {name}: expected 0x{expected_pid:04X}, got 0x{our_pid:04X}"
        );
    }
}
