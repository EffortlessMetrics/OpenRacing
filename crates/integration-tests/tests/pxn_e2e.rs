//! BDD end-to-end tests for the PXN / Lite Star protocol stack.
//!
//! Each test follows a Given/When/Then pattern to verify observable
//! hardware-ready behaviors without real USB hardware.

use racing_wheel_hid_pxn_protocol::{
    PRODUCT_GT987, PRODUCT_V10, PRODUCT_V12, PRODUCT_V12_LITE, PRODUCT_V12_LITE_2, VENDOR_ID,
    is_pxn, product_name,
};

// ─── Scenario 1: product ID constants match expected values ──────────────────

#[test]
fn given_product_id_constants_then_values_match_specification()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(VENDOR_ID, 0x11FF, "PXN VID must be 0x11FF");
    assert_eq!(PRODUCT_V10, 0x3245, "V10 PID must be 0x3245");
    assert_eq!(PRODUCT_V12, 0x1212, "V12 PID must be 0x1212");
    assert_eq!(PRODUCT_V12_LITE, 0x1112, "V12 Lite PID must be 0x1112");
    assert_eq!(PRODUCT_V12_LITE_2, 0x1211, "V12 Lite 2 PID must be 0x1211");
    assert_eq!(PRODUCT_GT987, 0x2141, "GT987 PID must be 0x2141");

    Ok(())
}

// ─── Scenario 2: is_pxn correctly identifies known and unknown devices ───────

#[test]
fn given_vid_pid_pairs_when_checked_then_known_devices_recognised()
-> Result<(), Box<dyn std::error::Error>> {
    assert!(is_pxn(VENDOR_ID, PRODUCT_V10));
    assert!(is_pxn(VENDOR_ID, PRODUCT_V12));
    assert!(is_pxn(VENDOR_ID, PRODUCT_V12_LITE));
    assert!(is_pxn(VENDOR_ID, PRODUCT_V12_LITE_2));
    assert!(is_pxn(VENDOR_ID, PRODUCT_GT987));

    // Wrong vendor ID is rejected
    assert!(!is_pxn(0x0000, PRODUCT_V10));
    assert!(!is_pxn(0xFFFF, PRODUCT_V12));

    // Unknown PID under correct VID is rejected
    assert!(!is_pxn(VENDOR_ID, 0x0000));
    assert!(!is_pxn(VENDOR_ID, 0xFFFF));

    Ok(())
}

// ─── Scenario 3: product_name returns correct human-readable strings ─────────

#[test]
fn given_known_pids_when_named_then_human_readable_strings_returned()
-> Result<(), Box<dyn std::error::Error>> {
    assert_eq!(product_name(PRODUCT_V10), Some("PXN V10"));
    assert_eq!(product_name(PRODUCT_V12), Some("PXN V12"));
    assert_eq!(product_name(PRODUCT_V12_LITE), Some("PXN V12 Lite"));
    assert_eq!(product_name(PRODUCT_V12_LITE_2), Some("PXN V12 Lite (SE)"));
    assert_eq!(product_name(PRODUCT_GT987), Some("Lite Star GT987 FF"));
    assert_eq!(product_name(0xFFFF), None);

    Ok(())
}

// ─── Scenario 4: engine dispatch routes PXN devices ──────────────────────────

#[test]
fn given_pxn_vid_pid_when_dispatched_then_handler_returned()
-> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::get_vendor_protocol;

    let cases: &[(u16, &str)] = &[
        (PRODUCT_V10, "PXN V10"),
        (PRODUCT_V12, "PXN V12"),
        (PRODUCT_V12_LITE, "PXN V12 Lite"),
        (PRODUCT_V12_LITE_2, "PXN V12 Lite (SE)"),
        (PRODUCT_GT987, "Lite Star GT987"),
    ];

    for (pid, label) in cases {
        let proto = get_vendor_protocol(VENDOR_ID, *pid);
        assert!(
            proto.is_some(),
            "{label} (VID 0x{:04X}, PID 0x{pid:04X}) must dispatch",
            VENDOR_ID,
        );
    }

    Ok(())
}

// ─── Scenario 5: unknown PID on PXN VID returns None ─────────────────────────

#[test]
fn given_unknown_pid_on_pxn_vid_when_dispatched_then_none()
-> Result<(), Box<dyn std::error::Error>> {
    use racing_wheel_engine::hid::vendor::get_vendor_protocol;

    let proto = get_vendor_protocol(VENDOR_ID, 0x0001);
    assert!(
        proto.is_none(),
        "unknown PID on PXN VID must return None"
    );

    Ok(())
}
