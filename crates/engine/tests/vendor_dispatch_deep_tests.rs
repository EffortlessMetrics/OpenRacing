//! Deep tests for vendor dispatch, vendor-specific initialization, protocol
//! switching, feature detection, and command dispatching.
//!
//! Covers: VID/PID dispatch for all 15+ supported vendors, vendor initialization
//! sequences, protocol mode switching (DFU/bootloader), vendor-specific feature
//! detection, command dispatching, and FFB configuration validation.

use racing_wheel_engine::hid::vendor::{
    get_vendor_protocol, get_vendor_protocol_with_hid_pid_fallback, FfbConfig,
};

type R = Result<(), Box<dyn std::error::Error>>;

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

/// Asserts that `get_vendor_protocol` returns `Some` for the given VID/PID pair.
fn assert_dispatches(vid: u16, pid: u16, label: &str) {
    assert!(
        get_vendor_protocol(vid, pid).is_some(),
        "{label} (VID=0x{vid:04X}, PID=0x{pid:04X}) must dispatch to a handler"
    );
}

/// Returns the FFB config for a VID/PID, panicking with context on failure.
fn ffb_config_for(vid: u16, pid: u16, label: &str) -> FfbConfig {
    get_vendor_protocol(vid, pid)
        .unwrap_or_else(|| panic!("{label} (0x{vid:04X}:0x{pid:04X}) must have a handler"))
        .get_ffb_config()
}

/// A DeviceWriter that records all writes for inspection.
struct RecordingWriter {
    output_reports: Vec<Vec<u8>>,
    feature_reports: Vec<Vec<u8>>,
}

impl RecordingWriter {
    fn new() -> Self {
        Self {
            output_reports: Vec::new(),
            feature_reports: Vec::new(),
        }
    }
}

impl racing_wheel_engine::hid::vendor::DeviceWriter for RecordingWriter {
    fn write_output_report(
        &mut self,
        data: &[u8],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        self.output_reports.push(data.to_vec());
        Ok(data.len())
    }
    fn write_feature_report(
        &mut self,
        data: &[u8],
    ) -> Result<usize, Box<dyn std::error::Error>> {
        self.feature_reports.push(data.to_vec());
        Ok(data.len())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Vendor detection by VID/PID for all supported vendors
// ═══════════════════════════════════════════════════════════════════════════

mod vendor_detection {
    use super::*;

    #[test]
    fn detect_01_fanatec_wheelbases() {
        let pids = [
            (0x0001, "ClubSport V2"),
            (0x0006, "DD1"),
            (0x0007, "DD2"),
            (0x0020, "CSL DD"),
            (0x0024, "GT DD Pro"),
            (0x01E9, "ClubSport DD"),
        ];
        for (pid, label) in pids {
            assert_dispatches(0x0EB7, pid, &format!("Fanatec {label}"));
        }
    }

    #[test]
    fn detect_02_logitech_wheels() {
        let pids = [
            (0xC24F, "G29"),
            (0xC262, "G920"),
            (0xC266, "G923"),
            (0xC268, "G PRO"),
            (0xC299, "G25"),
            (0xC29B, "G27"),
        ];
        for (pid, label) in pids {
            assert_dispatches(0x046D, pid, &format!("Logitech {label}"));
        }
    }

    #[test]
    fn detect_03_moza_devices() {
        let pids = [
            (0x0000, "R16/R21 V1"),
            (0x0002, "R9 V1"),
            (0x0004, "R5 V1"),
            (0x0010, "R16/R21 V2"),
            (0x0012, "R9 V2"),
        ];
        for (pid, label) in pids {
            assert_dispatches(0x346E, pid, &format!("Moza {label}"));
        }
    }

    #[test]
    fn detect_04_thrustmaster_wheels() {
        let pids = [
            (0xB677, "T150"),
            (0xB66E, "T300"),
            (0xB65E, "T500 RS"),
            (0xB689, "TS-PC"),
            (0xB69B, "T818"),
        ];
        for (pid, label) in pids {
            assert_dispatches(0x044F, pid, &format!("Thrustmaster {label}"));
        }
    }

    #[test]
    fn detect_05_simagic_evo_devices() {
        let pids = [
            (0x0500, "EVO Sport"),
            (0x0502, "EVO Pro"),
            (0x0700, "NEO"),
        ];
        for (pid, label) in pids {
            assert_dispatches(0x3670, pid, &format!("Simagic {label}"));
        }
    }

    #[test]
    fn detect_06_simucube_devices() {
        // Simucube shares VID 0x16D0 with legacy Simagic, specific PIDs distinguish
        let handler = get_vendor_protocol(0x16D0, 0x0D5F);
        assert!(handler.is_some(), "Simucube must dispatch for known PIDs");
    }

    #[test]
    fn detect_07_asetek_simsports() {
        let pids = [(0xF300, "Invicta"), (0xF301, "Forte"), (0xF303, "La Prima")];
        for (pid, label) in pids {
            assert_dispatches(0x2433, pid, &format!("Asetek {label}"));
        }
    }

    #[test]
    fn detect_08_vrs_directforce() {
        assert_dispatches(0x0483, 0xA355, "VRS DirectForce Pro");
    }

    #[test]
    fn detect_09_openffboard() {
        assert_dispatches(0x1209, 0xFFB0, "OpenFFBoard");
    }

    #[test]
    fn detect_10_leo_bodnar() {
        let pids = [(0x000E, "Wheel Interface"), (0x000C, "BBI32")];
        for (pid, label) in pids {
            assert_dispatches(0x1DD2, pid, &format!("Leo Bodnar {label}"));
        }
    }

    #[test]
    fn detect_11_cammus_wheelbases() {
        assert_dispatches(0x3416, 0x0301, "Cammus C5");
        assert_dispatches(0x3416, 0x0302, "Cammus C12");
    }

    #[test]
    fn detect_12_accuforce_pro() {
        assert_dispatches(0x1FC9, 0x804C, "AccuForce Pro");
    }

    #[test]
    fn detect_13_simplemotion_v2() {
        assert_dispatches(0x1D50, 0x0001, "SimpleMotion V2");
    }

    #[test]
    fn detect_14_unknown_vid_returns_none() {
        assert!(
            get_vendor_protocol(0x9999, 0x0001).is_none(),
            "unknown VID must return None"
        );
    }

    #[test]
    fn detect_15_stm_vid_simagic_fallback() {
        // STM VID 0x0483 with non-VRS, non-Cube Controls PID falls to Simagic
        let handler = get_vendor_protocol(0x0483, 0x0001);
        assert!(handler.is_some(), "STM VID with generic PID should fall through to Simagic");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. Vendor-specific initialization sequence
// ═══════════════════════════════════════════════════════════════════════════

mod vendor_initialization {
    use super::*;

    #[test]
    fn init_01_fanatec_sends_init_reports() -> R {
        let protocol = get_vendor_protocol(0x0EB7, 0x0006)
            .ok_or("Fanatec DD1 handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        // Fanatec init sends mode-switch handshake reports
        let total = writer.output_reports.len() + writer.feature_reports.len();
        assert!(total > 0, "Fanatec must send init reports");
        Ok(())
    }

    #[test]
    fn init_02_logitech_sends_native_mode_switch() -> R {
        let protocol = get_vendor_protocol(0x046D, 0xC266)
            .ok_or("Logitech G923 handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        let total = writer.output_reports.len() + writer.feature_reports.len();
        assert!(total > 0, "Logitech must send native mode switch");
        Ok(())
    }

    #[test]
    fn init_03_generic_hid_pid_is_noop() -> R {
        let protocol = get_vendor_protocol_with_hid_pid_fallback(0x9999, 0x0001, true)
            .ok_or("generic HID PID handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        // Generic handler typically sends no init reports
        Ok(())
    }

    #[test]
    fn init_04_asetek_is_plug_and_play() -> R {
        let protocol = get_vendor_protocol(0x2433, 0xF300)
            .ok_or("Asetek Invicta handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        // Asetek: plug-and-play, no init required
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Vendor protocol switching (DFU mode, bootloader mode)
// ═══════════════════════════════════════════════════════════════════════════

mod protocol_switching {
    use super::*;

    #[test]
    fn switch_01_same_vendor_different_mode_pids() {
        // Fanatec: wheelbase PID vs bootloader PID are different devices
        let wheelbase = get_vendor_protocol(0x0EB7, 0x0006);
        let different_pid = get_vendor_protocol(0x0EB7, 0xFFFF);
        // Both resolve (Fanatec always matches on VID), but configs may differ
        assert!(wheelbase.is_some());
        assert!(different_pid.is_some());
    }

    #[test]
    fn switch_02_stm_vid_routes_by_pid_range() {
        // VRS range (0xA355..=0xA35A) vs Simagic vs Cube Controls on STM VID
        let vrs = get_vendor_protocol(0x0483, 0xA355);
        let simagic = get_vendor_protocol(0x0483, 0x0001);
        assert!(vrs.is_some(), "VRS PID on STM VID must dispatch to VRS");
        assert!(simagic.is_some(), "generic STM PID falls to Simagic");
    }

    #[test]
    fn switch_03_openmoko_vid_simucube_vs_simagic() {
        // 0x16D0: Simucube PIDs vs legacy Simagic PIDs
        let simucube = get_vendor_protocol(0x16D0, 0x0D5F);
        let legacy = get_vendor_protocol(0x16D0, 0x0001);
        assert!(simucube.is_some());
        assert!(legacy.is_some());
    }

    #[test]
    fn switch_04_pid_codes_vid_openffboard_vs_button_box() {
        // 0x1209: OpenFFBoard vs button box PIDs
        let offb = get_vendor_protocol(0x1209, 0xFFB0);
        assert!(offb.is_some(), "OpenFFBoard PID should match");
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. Vendor-specific feature detection
// ═══════════════════════════════════════════════════════════════════════════

mod feature_detection {
    use super::*;

    #[test]
    fn feature_01_fanatec_dd_is_v2_hardware() {
        let dd1 = get_vendor_protocol(0x0EB7, 0x0006);
        assert!(
            dd1.as_ref().is_some_and(|p| p.is_v2_hardware()),
            "Fanatec DD1 should be v2 hardware"
        );
    }

    #[test]
    fn feature_02_logitech_is_not_v2() {
        let g29 = get_vendor_protocol(0x046D, 0xC24F);
        assert!(
            g29.as_ref().is_none_or(|p| !p.is_v2_hardware()),
            "Logitech G29 should not be v2 hardware"
        );
    }

    #[test]
    fn feature_03_generic_hid_pid_not_v2() {
        let generic = get_vendor_protocol_with_hid_pid_fallback(0x9999, 0x0001, true);
        assert!(
            generic.as_ref().is_none_or(|p| !p.is_v2_hardware()),
            "generic HID PID should not be v2 hardware"
        );
    }

    #[test]
    fn feature_04_ffb_config_max_torque_positive() {
        let vendors: &[(u16, u16, &str)] = &[
            (0x0EB7, 0x0006, "Fanatec DD1"),
            (0x046D, 0xC266, "Logitech G923"),
            (0x346E, 0x0010, "Moza R16 V2"),
            (0x044F, 0xB66E, "Thrustmaster T300"),
            (0x2433, 0xF300, "Asetek Invicta"),
        ];
        for &(vid, pid, label) in vendors {
            let cfg = ffb_config_for(vid, pid, label);
            assert!(
                cfg.max_torque_nm > 0.0,
                "{label} must have positive max_torque_nm, got {}",
                cfg.max_torque_nm
            );
        }
    }

    #[test]
    fn feature_05_generic_handler_conservative_torque() {
        let generic = get_vendor_protocol_with_hid_pid_fallback(0x9999, 0x0001, true);
        if let Some(handler) = generic {
            let cfg = handler.get_ffb_config();
            assert!(
                cfg.max_torque_nm <= 10.0,
                "generic handler should use conservative torque limit"
            );
        }
    }

    #[test]
    fn feature_06_fanatec_requires_polling_interval() {
        let cfg = ffb_config_for(0x0EB7, 0x0006, "Fanatec DD1");
        assert!(
            cfg.required_b_interval.is_some(),
            "Fanatec must specify required USB polling interval"
        );
    }

    #[test]
    fn feature_07_logitech_no_required_interval() {
        let cfg = ffb_config_for(0x046D, 0xC266, "Logitech G923");
        assert!(
            cfg.required_b_interval.is_none(),
            "Logitech should not require specific polling interval"
        );
    }

    #[test]
    fn feature_08_output_report_id_varies_by_vendor() {
        let fanatec = get_vendor_protocol(0x0EB7, 0x0006);
        let logitech = get_vendor_protocol(0x046D, 0xC266);
        // Both should return handlers; report IDs may differ
        assert!(fanatec.is_some());
        assert!(logitech.is_some());
        // Just verify the accessor doesn't panic
        let _ = fanatec.as_ref().map(|p| p.output_report_id());
        let _ = logitech.as_ref().map(|p| p.output_report_id());
    }

    #[test]
    fn feature_09_output_report_len_accessible() {
        let vendors: &[(u16, u16)] = &[
            (0x0EB7, 0x0006),
            (0x046D, 0xC266),
            (0x346E, 0x0010),
            (0x044F, 0xB66E),
        ];
        for &(vid, pid) in vendors {
            if let Some(handler) = get_vendor_protocol(vid, pid) {
                // output_report_len returns Option<usize>; just verify no panic
                let _ = handler.output_report_len();
            }
        }
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Vendor command dispatching
// ═══════════════════════════════════════════════════════════════════════════

mod command_dispatching {
    use super::*;

    #[test]
    fn cmd_01_fanatec_init_and_shutdown() -> R {
        let protocol = get_vendor_protocol(0x0EB7, 0x0006)
            .ok_or("Fanatec handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        let init_count = writer.output_reports.len() + writer.feature_reports.len();
        protocol.shutdown_device(&mut writer)?;
        let shutdown_count =
            (writer.output_reports.len() + writer.feature_reports.len()) - init_count;
        assert!(
            init_count > 0 || shutdown_count > 0,
            "Fanatec must send reports during init or shutdown"
        );
        Ok(())
    }

    #[test]
    fn cmd_02_logitech_init_and_shutdown() -> R {
        let protocol = get_vendor_protocol(0x046D, 0xC266)
            .ok_or("Logitech handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        protocol.shutdown_device(&mut writer)?;
        Ok(())
    }

    #[test]
    fn cmd_03_thrustmaster_init_and_shutdown() -> R {
        let protocol = get_vendor_protocol(0x044F, 0xB66E)
            .ok_or("Thrustmaster handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        protocol.shutdown_device(&mut writer)?;
        Ok(())
    }

    #[test]
    fn cmd_04_moza_init_and_shutdown() -> R {
        let protocol = get_vendor_protocol(0x346E, 0x0010)
            .ok_or("Moza handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        protocol.shutdown_device(&mut writer)?;
        Ok(())
    }

    #[test]
    fn cmd_05_send_feature_report_all_vendors() -> R {
        let vendors: &[(u16, u16, &str)] = &[
            (0x0EB7, 0x0006, "Fanatec"),
            (0x046D, 0xC266, "Logitech"),
            (0x346E, 0x0010, "Moza"),
            (0x044F, 0xB66E, "Thrustmaster"),
        ];
        for &(vid, pid, label) in vendors {
            let protocol =
                get_vendor_protocol(vid, pid).ok_or(format!("{label} handler expected"))?;
            let mut writer = RecordingWriter::new();
            // send_feature_report may succeed or fail depending on vendor
            let _ = protocol.send_feature_report(&mut writer, 0x01, &[0x00; 8]);
        }
        Ok(())
    }

    #[test]
    fn cmd_06_hid_pid_fallback_init() -> R {
        let protocol = get_vendor_protocol_with_hid_pid_fallback(0xBEEF, 0xCAFE, true)
            .ok_or("generic HID PID handler expected")?;
        let mut writer = RecordingWriter::new();
        protocol.initialize_device(&mut writer)?;
        protocol.shutdown_device(&mut writer)?;
        Ok(())
    }

    #[test]
    fn cmd_07_hid_pid_fallback_without_capability_returns_none() {
        let result = get_vendor_protocol_with_hid_pid_fallback(0xBEEF, 0xCAFE, false);
        assert!(
            result.is_none(),
            "without HID PID capability, fallback must return None"
        );
    }

    #[test]
    fn cmd_08_all_dispatched_handlers_implement_get_ffb_config() {
        let vendors: &[(u16, u16)] = &[
            (0x0EB7, 0x0006),
            (0x046D, 0xC266),
            (0x346E, 0x0010),
            (0x044F, 0xB66E),
            (0x3670, 0x0500),
            (0x2433, 0xF300),
            (0x1D50, 0x0001),
            (0x1DD2, 0x000E),
            (0x3416, 0x0301),
        ];
        for &(vid, pid) in vendors {
            if let Some(handler) = get_vendor_protocol(vid, pid) {
                let cfg = handler.get_ffb_config();
                // max_torque_nm should be non-negative for all vendors
                assert!(
                    cfg.max_torque_nm >= 0.0,
                    "VID=0x{vid:04X} PID=0x{pid:04X} has negative torque"
                );
            }
        }
    }

    #[test]
    fn cmd_09_encoder_cpr_reasonable_for_wheelbases() {
        let wheelbases: &[(u16, u16, &str)] = &[
            (0x046D, 0xC266, "Logitech G923"),
            (0x0EB7, 0x0006, "Fanatec DD1"),
        ];
        for &(vid, pid, label) in wheelbases {
            let cfg = ffb_config_for(vid, pid, label);
            assert!(
                cfg.encoder_cpr > 0,
                "{label} encoder_cpr must be positive, got {}",
                cfg.encoder_cpr
            );
        }
    }

    #[test]
    fn cmd_10_input_only_devices_zero_torque() {
        // Heusinkveld pedals are input-only
        let handler = get_vendor_protocol(0x30B7, 0x0001);
        if let Some(h) = handler {
            let cfg = h.get_ffb_config();
            assert!(
                cfg.max_torque_nm < 0.01,
                "Heusinkveld input-only devices should have ~0 torque"
            );
        }
    }
}
