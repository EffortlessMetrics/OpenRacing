//! BDD end-to-end tests for the AccuForce HID protocol crate.
//!
//! Each test follows a Given/When/Then pattern to verify device classification,
//! model identification, and protocol constants without real USB hardware.

use racing_wheel_hid_accuforce_protocol::{
    AccuForceModel, DeviceInfo, HID_PID_USAGE_PAGE, MAX_REPORT_BYTES, PID_ACCUFORCE_PRO,
    RECOMMENDED_B_INTERVAL_MS, VENDOR_ID, is_accuforce, is_accuforce_pid,
};

// ─── Scenario 1: AccuForce Pro recognised by VID/PID pair ─────────────────────

#[test]
fn scenario_device_classification_given_accuforce_vid_pid_when_checked_then_recognised() {
    // Given: the AccuForce Pro vendor and product IDs
    let vid = VENDOR_ID;
    let pid = PID_ACCUFORCE_PRO;

    // When: checking device classification
    let result = is_accuforce(vid, pid);

    // Then: the device is recognised as an AccuForce
    assert!(
        result,
        "VID 0x{vid:04X} + PID 0x{pid:04X} must be recognised"
    );
}

// ─── Scenario 2: known PID recognised without VID check ──────────────────────

#[test]
fn scenario_pid_check_given_accuforce_pro_pid_when_checked_then_recognised() {
    // Given: the AccuForce Pro product ID
    let pid = PID_ACCUFORCE_PRO;

    // When: checking PID only
    let result = is_accuforce_pid(pid);

    // Then: recognised
    assert!(
        result,
        "PID 0x{pid:04X} must be recognised by is_accuforce_pid"
    );
}

// ─── Scenario 3: wrong vendor ID rejects device ──────────────────────────────

#[test]
fn scenario_device_classification_given_wrong_vid_when_checked_then_rejected() {
    // Given: known non-AccuForce vendor IDs
    let foreign_vids: &[u16] = &[0x0000, 0x16D0, 0x1DD2, 0x2433];

    for &vid in foreign_vids {
        // When: checking with AccuForce Pro PID but wrong VID
        let result = is_accuforce(vid, PID_ACCUFORCE_PRO);

        // Then: rejected
        assert!(
            !result,
            "VID 0x{vid:04X} with AccuForce PID must be rejected"
        );
    }
}

// ─── Scenario 4: unknown PIDs are not recognised ─────────────────────────────

#[test]
fn scenario_pid_check_given_unknown_pid_when_checked_then_rejected() {
    // Given: PIDs that are not AccuForce products
    let unknown_pids: &[u16] = &[0x0000, 0x0001, 0x1234, 0xDEAD, 0xFFFF];

    for &pid in unknown_pids {
        // When: checking PID
        let result = is_accuforce_pid(pid);

        // Then: not recognised
        assert!(!result, "PID 0x{pid:04X} must not be recognised");
    }
}

// ─── Scenario 5: Pro model resolved from product ID ─────────────────────────

#[test]
fn scenario_model_id_given_pro_pid_when_resolved_then_returns_pro() {
    // Given: the AccuForce Pro product ID
    let pid = PID_ACCUFORCE_PRO;

    // When: resolving model
    let model = AccuForceModel::from_product_id(pid);

    // Then: returns Pro variant
    assert_eq!(model, AccuForceModel::Pro);
}

// ─── Scenario 6: unknown PID resolves to Unknown model ──────────────────────

#[test]
fn scenario_model_id_given_unknown_pid_when_resolved_then_returns_unknown() {
    // Given: PIDs that are not AccuForce products
    let unknown_pids: &[u16] = &[0x0000, 0x0001, 0xFFFF];

    for &pid in unknown_pids {
        // When: resolving model
        let model = AccuForceModel::from_product_id(pid);

        // Then: returns Unknown variant
        assert_eq!(
            model,
            AccuForceModel::Unknown,
            "PID 0x{pid:04X} must resolve to Unknown"
        );
    }
}

// ─── Scenario 7: Pro display name is human-readable ─────────────────────────

#[test]
fn scenario_display_name_given_pro_model_when_queried_then_contains_accuforce() {
    // Given: the Pro model
    let model = AccuForceModel::Pro;

    // When: getting display name
    let name = model.display_name();

    // Then: contains "AccuForce" and "Pro", is non-empty
    assert!(!name.is_empty(), "display name must not be empty");
    assert!(
        name.contains("AccuForce"),
        "Pro display name must contain 'AccuForce', got: {name}"
    );
    assert!(
        name.contains("Pro"),
        "Pro display name must contain 'Pro', got: {name}"
    );
}

// ─── Scenario 8: Unknown model display name is descriptive ──────────────────

#[test]
fn scenario_display_name_given_unknown_model_when_queried_then_non_empty() {
    // Given: the Unknown model
    let model = AccuForceModel::Unknown;

    // When: getting display name
    let name = model.display_name();

    // Then: non-empty and indicates unknown status
    assert!(!name.is_empty(), "Unknown display name must not be empty");
    assert!(
        name.contains("unknown") || name.contains("Unknown"),
        "Unknown display name should indicate unknown status, got: {name}"
    );
}

// ─── Scenario 9: Pro max torque is 7.0 Nm ───────────────────────────────────

#[test]
fn scenario_max_torque_given_pro_model_when_queried_then_returns_7nm() {
    // Given: the Pro model
    let model = AccuForceModel::Pro;

    // When: querying max torque
    let torque = model.max_torque_nm();

    // Then: returns 7.0 Nm
    assert!(
        (torque - 7.0).abs() < f32::EPSILON,
        "Pro max torque must be 7.0 Nm, got: {torque}"
    );
}

// ─── Scenario 10: Unknown model max torque is positive ──────────────────────

#[test]
fn scenario_max_torque_given_unknown_model_when_queried_then_positive() {
    // Given: the Unknown model
    let model = AccuForceModel::Unknown;

    // When: querying max torque
    let torque = model.max_torque_nm();

    // Then: positive (safe default)
    assert!(
        torque > 0.0,
        "Unknown max torque must be positive, got: {torque}"
    );
}

// ─── Scenario 11: vendor ID constant matches NXP ────────────────────────────

#[test]
fn scenario_constants_given_vendor_id_when_checked_then_matches_nxp() {
    // Given/When: the VENDOR_ID constant
    // Then: matches NXP Semiconductors USB VID
    assert_eq!(VENDOR_ID, 0x1FC9, "VENDOR_ID must be NXP USB VID 0x1FC9");
}

// ─── Scenario 12: AccuForce Pro PID constant value ──────────────────────────

#[test]
fn scenario_constants_given_pro_pid_when_checked_then_matches_known_value() {
    // Given/When: the PID_ACCUFORCE_PRO constant
    // Then: matches known AccuForce Pro PID
    assert_eq!(
        PID_ACCUFORCE_PRO, 0x804C,
        "PID_ACCUFORCE_PRO must be 0x804C"
    );
}

// ─── Scenario 13: DeviceInfo from VID/PID resolves model correctly ──────────

#[test]
fn scenario_device_info_given_pro_vid_pid_when_constructed_then_fields_correct()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: AccuForce Pro VID/PID
    let vid = VENDOR_ID;
    let pid = PID_ACCUFORCE_PRO;

    // When: constructing DeviceInfo
    let info = DeviceInfo::from_vid_pid(vid, pid);

    // Then: all fields are correctly populated
    assert_eq!(info.vendor_id, vid);
    assert_eq!(info.product_id, pid);
    assert_eq!(info.model, AccuForceModel::Pro);

    Ok(())
}

// ─── Scenario 14: DeviceInfo with unknown PID ───────────────────────────────

#[test]
fn scenario_device_info_given_unknown_pid_when_constructed_then_model_unknown()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: AccuForce VID but unknown PID
    let vid = VENDOR_ID;
    let pid = 0xFFFF;

    // When: constructing DeviceInfo
    let info = DeviceInfo::from_vid_pid(vid, pid);

    // Then: model is Unknown, IDs are preserved
    assert_eq!(info.vendor_id, vid);
    assert_eq!(info.product_id, pid);
    assert_eq!(info.model, AccuForceModel::Unknown);

    Ok(())
}

// ─── Scenario 15: DeviceInfo model matches is_accuforce_pid ─────────────────

#[test]
fn scenario_device_info_given_any_pid_when_constructed_then_model_agrees_with_pid_check() {
    // Given: all known PIDs
    let pids: &[u16] = &[PID_ACCUFORCE_PRO, 0x0000, 0xFFFF];

    for &pid in pids {
        // When: constructing DeviceInfo
        let info = DeviceInfo::from_vid_pid(VENDOR_ID, pid);

        // Then: model is non-Unknown iff is_accuforce_pid returns true
        let is_known = is_accuforce_pid(pid);
        let is_named = info.model != AccuForceModel::Unknown;
        assert_eq!(
            is_known, is_named,
            "PID 0x{pid:04X}: is_accuforce_pid={is_known} but model is {:?}",
            info.model
        );
    }
}

// ─── Scenario 16: HID PID usage page constant ──────────────────────────────

#[test]
fn scenario_constants_given_hid_pid_usage_page_when_checked_then_matches_spec() {
    // Given/When: HID PID usage page constant
    // Then: matches USB HID PID spec (0x000F = Physical Interface Device)
    assert_eq!(
        HID_PID_USAGE_PAGE, 0x000F,
        "HID PID usage page must be 0x000F per USB HID spec"
    );
}

// ─── Scenario 17: MAX_REPORT_BYTES within USB full-speed limit ──────────────

#[test]
fn scenario_constants_given_max_report_bytes_when_checked_then_within_usb_limit() {
    // Given/When: MAX_REPORT_BYTES constant
    // Then: must not exceed USB full-speed HID limit of 64 bytes
    const { assert!(MAX_REPORT_BYTES <= 64) };
    assert_eq!(MAX_REPORT_BYTES, 64, "MAX_REPORT_BYTES must be exactly 64");
}

// ─── Scenario 18: recommended bInterval is positive ─────────────────────────

#[test]
fn scenario_constants_given_b_interval_when_checked_then_positive() {
    // Given/When: RECOMMENDED_B_INTERVAL_MS constant
    // Then: positive and reasonable for 100-200 Hz update rate
    const { assert!(RECOMMENDED_B_INTERVAL_MS > 0) };
    const { assert!(RECOMMENDED_B_INTERVAL_MS <= 20) };
}

// ─── Scenario 19: all AccuForceModel variants have consistent properties ────

#[test]
fn scenario_all_models_given_every_variant_when_queried_then_have_valid_properties() {
    // Given: every known AccuForceModel variant
    let variants = [AccuForceModel::Pro, AccuForceModel::Unknown];

    for model in variants {
        // When/Then: display name is non-empty
        assert!(
            !model.display_name().is_empty(),
            "{model:?} display name must not be empty"
        );

        // When/Then: max torque is finite and positive
        let torque = model.max_torque_nm();
        assert!(
            torque.is_finite() && torque > 0.0,
            "{model:?} max torque must be finite and positive, got: {torque}"
        );
    }
}

// ─── Scenario 20: DeviceInfo equality semantics ─────────────────────────────

#[test]
fn scenario_device_info_given_same_vid_pid_when_compared_then_equal()
-> Result<(), Box<dyn std::error::Error>> {
    // Given: two DeviceInfo instances from same VID/PID
    let a = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);
    let b = DeviceInfo::from_vid_pid(VENDOR_ID, PID_ACCUFORCE_PRO);

    // Then: they are equal
    assert_eq!(a, b, "DeviceInfo with same VID/PID must be equal");

    // Given: DeviceInfo with different PID
    let c = DeviceInfo::from_vid_pid(VENDOR_ID, 0xFFFF);

    // Then: not equal to Pro
    assert_ne!(a, c, "DeviceInfo with different PID must not be equal");

    Ok(())
}
