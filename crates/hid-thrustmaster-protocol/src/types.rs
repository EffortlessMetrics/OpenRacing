//! Thrustmaster device types: models, categories, and normalization.

#![deny(static_mut_refs)]

use crate::ids::product_ids;

#[allow(unused_imports)]
use crate::ids::Model;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThrustmasterDeviceCategory {
    Wheelbase,
    Pedals,
    Shifter,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThrustmasterDeviceIdentity {
    pub product_id: u16,
    pub name: &'static str,
    pub category: ThrustmasterDeviceCategory,
    pub supports_ffb: bool,
}

pub fn identify_device(product_id: u16) -> ThrustmasterDeviceIdentity {
    match product_id {
        // T150/TMX: separate protocol family from T300RS.
        // Uses scarburato/t150_driver FFB protocol (0x40/0x41/0x43 commands).
        // See Model::T150 and ProtocolFamily::T150 for protocol details.
        product_ids::T150 | product_ids::TMX => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T150",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: true,
        },
        // T300RS family: all share the hid-tmff2 Report ID 0x60 protocol.
        product_ids::T300_RS
        | product_ids::T300_RS_PS4
        | product_ids::T300_RS_GT
        | product_ids::TX_RACING
        | product_ids::TX_RACING_ORIG => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T300 RS",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: true,
        },
        // T500RS: older protocol, no community FFB driver exists.
        // Init switch value 0x0002; FFB wire format is undocumented.
        // See Model::T500RS and ProtocolFamily::T500 for details.
        product_ids::T500_RS => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T500 RS",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: true,
        },
        product_ids::T248 | product_ids::T248X => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T248",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: true,
        },
        product_ids::TS_PC_RACER => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster TS-PC Racer",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: true,
        },
        product_ids::TS_XW | product_ids::TS_XW_GIP => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster TS-XW",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: true,
        },
        product_ids::T818 => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T818",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: true,
        },
        product_ids::T80 | product_ids::T80_FERRARI_488 => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T80 Racing Wheel",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: false,
        },
        product_ids::NASCAR_PRO_FF2
        | product_ids::FGT_RUMBLE_FORCE
        | product_ids::RGT_FF_CLUTCH
        | product_ids::FGT_FORCE_FEEDBACK
        | product_ids::F430_FORCE_FEEDBACK => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster Legacy Wheel",
            category: ThrustmasterDeviceCategory::Wheelbase,
            supports_ffb: true,
        },
        _ => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster Unknown",
            category: ThrustmasterDeviceCategory::Unknown,
            supports_ffb: false,
        },
    }
}

pub fn is_wheel_product(product_id: u16) -> bool {
    matches!(
        identify_device(product_id).category,
        ThrustmasterDeviceCategory::Wheelbase
    )
}

pub fn is_pedal_product(product_id: u16) -> bool {
    matches!(
        identify_device(product_id).category,
        ThrustmasterDeviceCategory::Pedals
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThrustmasterPedalAxesRaw {
    pub throttle: u8,
    pub brake: u8,
    pub clutch: Option<u8>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ThrustmasterPedalAxes {
    pub throttle: f32,
    pub brake: f32,
    pub clutch: Option<f32>,
}

impl ThrustmasterPedalAxesRaw {
    pub fn normalize(self) -> ThrustmasterPedalAxes {
        const MAX: f32 = 255.0;
        ThrustmasterPedalAxes {
            throttle: self.throttle as f32 / MAX,
            brake: self.brake as f32 / MAX,
            clutch: self.clutch.map(|v| v as f32 / MAX),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_t300() {
        let identity = identify_device(product_ids::T300_RS);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
        assert!(identity.supports_ffb);
    }

    #[test]
    fn test_identify_t818() {
        let identity = identify_device(product_ids::T818);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Wheelbase);
        assert!(identity.supports_ffb);
        assert!((identity.name.to_string().contains("T818")));
    }

    #[test]
    fn test_identify_unknown_pid_is_not_pedals() {
        // Pedal PIDs were removed (incorrectly attributed to flight peripherals).
        // Unknown PIDs now fall through to Unknown category.
        let identity = identify_device(0xFFFF);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Unknown);
        assert!(!identity.supports_ffb);
    }

    #[test]
    fn test_is_wheel_product() {
        assert!(is_wheel_product(product_ids::TS_XW));
        assert!(is_wheel_product(product_ids::T300_RS));
        assert!(is_wheel_product(product_ids::T818));
        assert!(!is_wheel_product(0xFFFF)); // unknown PID is not a wheel
    }

    #[test]
    fn test_is_pedal_product() {
        // Pedal PIDs removed (were misattributed flight peripherals).
        // is_pedal_product now only returns true for PIDs explicitly matched.
        assert!(!is_pedal_product(0xFFFF));
        assert!(!is_pedal_product(product_ids::TS_XW));
    }

    #[test]
    fn test_model_from_pid() {
        assert_eq!(Model::from_product_id(product_ids::TS_XW), Model::TSXW);
        assert_eq!(Model::from_product_id(product_ids::T818), Model::T818);
        assert_eq!(Model::from_product_id(product_ids::T248X), Model::T248X);
    }

    #[test]
    fn test_model_max_torque() {
        assert!((Model::TGT.max_torque_nm() - 6.0).abs() < 0.01);
        assert!((Model::T818.max_torque_nm() - 10.0).abs() < 0.01);
        assert!((Model::T150.max_torque_nm() - 2.5).abs() < 0.01);
    }

    #[test]
    fn test_model_max_rotation() {
        assert_eq!(Model::TGT.max_rotation_deg(), 1080);
        assert_eq!(Model::T150.max_rotation_deg(), 1080);
    }

    #[test]
    fn test_pedal_normalize() {
        let raw = ThrustmasterPedalAxesRaw {
            throttle: 255,
            brake: 128,
            clutch: Some(64),
        };
        let normalized = raw.normalize();
        assert!((normalized.throttle - 1.0).abs() < 0.01);
        assert!((normalized.brake - 0.502).abs() < 0.01);
        assert!(normalized.clutch.is_some());
    }

    #[test]
    fn test_protocol_family_t300_group() {
        use crate::ids::ProtocolFamily;
        // All T300RS-family wheels share the same protocol
        assert_eq!(Model::T300RS.protocol_family(), ProtocolFamily::T300);
        assert_eq!(Model::T300RSPS4.protocol_family(), ProtocolFamily::T300);
        assert_eq!(Model::T300RSGT.protocol_family(), ProtocolFamily::T300);
        assert_eq!(Model::TXRacing.protocol_family(), ProtocolFamily::T300);
        assert_eq!(Model::T248.protocol_family(), ProtocolFamily::T300);
        assert_eq!(Model::TSPCRacer.protocol_family(), ProtocolFamily::T300);
        assert_eq!(Model::TSXW.protocol_family(), ProtocolFamily::T300);
        assert_eq!(Model::TGTII.protocol_family(), ProtocolFamily::T300);
    }

    #[test]
    fn test_protocol_family_t500_separate() {
        use crate::ids::ProtocolFamily;
        // T500RS uses a different, older protocol
        assert_eq!(Model::T500RS.protocol_family(), ProtocolFamily::T500);
        assert_ne!(Model::T500RS.protocol_family(), ProtocolFamily::T300);
    }

    #[test]
    fn test_protocol_family_t150_separate() {
        use crate::ids::ProtocolFamily;
        assert_eq!(Model::T150.protocol_family(), ProtocolFamily::T150);
        assert_eq!(Model::TMX.protocol_family(), ProtocolFamily::T150);
    }

    #[test]
    fn test_protocol_family_unknown_for_pedals() {
        use crate::ids::ProtocolFamily;
        assert_eq!(Model::T3PA.protocol_family(), ProtocolFamily::Unknown);
        assert_eq!(Model::TLCM.protocol_family(), ProtocolFamily::Unknown);
    }

    #[test]
    fn test_init_switch_values() {
        // T300RS family uses switch value 0x0005
        assert_eq!(Model::T300RS.init_switch_value(), Some(0x0005));
        assert_eq!(Model::T248.init_switch_value(), Some(0x0005));
        assert_eq!(Model::TSXW.init_switch_value(), Some(0x0005));
        // T500RS uses 0x0002
        assert_eq!(Model::T500RS.init_switch_value(), Some(0x0002));
        // T150/TMX use 0x0006
        assert_eq!(Model::T150.init_switch_value(), Some(0x0006));
        assert_eq!(Model::TMX.init_switch_value(), Some(0x0006));
        // Unknown models return None
        assert_eq!(Model::Unknown.init_switch_value(), None);
    }

    #[test]
    fn test_init_protocol_constants() {
        use crate::ids::init_protocol;
        assert_eq!(init_protocol::MODEL_QUERY_REQUEST, 73);
        assert_eq!(init_protocol::MODE_SWITCH_REQUEST, 83);
        assert_eq!(init_protocol::SETUP_INTERRUPTS.len(), 5);
        assert_eq!(init_protocol::KNOWN_MODELS.len(), 7);
        // Verify T500RS entry exists
        assert!(
            init_protocol::KNOWN_MODELS
                .iter()
                .any(|(model, switch, _)| *model == 0x0002 && *switch == 0x0002)
        );
    }
}
