//! Property-based tests for Moza device signature verification.
//!
//! Covers:
//! - Non-Moza VID always produces `Rejected`
//! - Moza VID verdict agrees with `identify_device` category
//! - `verify_signature` is deterministic
//! - Known PIDs produce correct verdict classes

use proptest::prelude::*;
use racing_wheel_hid_moza_protocol::{
    DeviceSignature, MozaDeviceCategory, SignatureVerdict, MOZA_VENDOR_ID, identify_device,
    product_ids, verify_signature,
};

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    /// Non-Moza VID always produces `Rejected` verdict for any PID.
    #[test]
    fn prop_non_moza_vid_always_rejected(vid in any::<u16>(), pid in any::<u16>()) {
        prop_assume!(vid != MOZA_VENDOR_ID);
        let sig = DeviceSignature::from_vid_pid(vid, pid);
        prop_assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::Rejected,
            "non-Moza VID 0x{:04X} must always produce Rejected", vid
        );
    }

    /// For Moza VID, verdict category agrees with `identify_device` category.
    #[test]
    fn prop_moza_vid_verdict_matches_category(pid in any::<u16>()) {
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        let verdict = verify_signature(&sig);
        let identity = identify_device(pid);

        match identity.category {
            MozaDeviceCategory::Wheelbase => {
                prop_assert_eq!(
                    verdict,
                    SignatureVerdict::KnownWheelbase,
                    "wheelbase PID 0x{:04X} must produce KnownWheelbase", pid
                );
            }
            MozaDeviceCategory::Pedals
            | MozaDeviceCategory::Shifter
            | MozaDeviceCategory::Handbrake => {
                prop_assert_eq!(
                    verdict,
                    SignatureVerdict::KnownPeripheral,
                    "peripheral PID 0x{:04X} must produce KnownPeripheral", pid
                );
            }
            MozaDeviceCategory::Unknown => {
                prop_assert!(
                    matches!(
                        verdict,
                        SignatureVerdict::KnownPeripheral | SignatureVerdict::UnknownProduct
                    ),
                    "unknown-category PID 0x{pid:04X} must be KnownPeripheral or UnknownProduct, got {verdict:?}"
                );
            }
        }
    }

    /// `verify_signature` is deterministic.
    #[test]
    fn prop_verify_signature_deterministic(vid: u16, pid: u16) {
        let sig = DeviceSignature::from_vid_pid(vid, pid);
        let a = verify_signature(&sig);
        let b = verify_signature(&sig);
        prop_assert_eq!(a, b, "verify_signature must be deterministic");
    }

    /// Known wheelbase PIDs with Moza VID always yield `KnownWheelbase`.
    #[test]
    fn prop_known_wheelbase_verdict(idx in 0usize..10usize) {
        let wheelbase_pids = [
            product_ids::R16_R21_V1,
            product_ids::R9_V1,
            product_ids::R5_V1,
            product_ids::R3_V1,
            product_ids::R12_V1,
            product_ids::R16_R21_V2,
            product_ids::R9_V2,
            product_ids::R5_V2,
            product_ids::R3_V2,
            product_ids::R12_V2,
        ];
        let pid = wheelbase_pids[idx];
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        prop_assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::KnownWheelbase,
            "wheelbase PID 0x{:04X} must produce KnownWheelbase", pid
        );
    }

    /// Known peripheral PIDs with Moza VID always yield `KnownPeripheral`.
    #[test]
    fn prop_known_peripheral_verdict(idx in 0usize..4usize) {
        let peripheral_pids = [
            product_ids::SR_P_PEDALS,
            product_ids::HGP_SHIFTER,
            product_ids::SGP_SHIFTER,
            product_ids::HBP_HANDBRAKE,
        ];
        let pid = peripheral_pids[idx];
        let sig = DeviceSignature::from_vid_pid(MOZA_VENDOR_ID, pid);
        prop_assert_eq!(
            verify_signature(&sig),
            SignatureVerdict::KnownPeripheral,
            "peripheral PID 0x{:04X} must produce KnownPeripheral", pid
        );
    }
}
