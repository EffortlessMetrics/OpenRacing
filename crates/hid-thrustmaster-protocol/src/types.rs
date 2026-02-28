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
        product_ids::T150 | product_ids::T150_PRO | product_ids::TMX => {
            ThrustmasterDeviceIdentity {
                product_id,
                name: "Thrustmaster T150",
                category: ThrustmasterDeviceCategory::Wheelbase,
                supports_ffb: true,
            }
        }
        product_ids::T300_RS
        | product_ids::T300_RS_PS4
        | product_ids::T300_RS_GT
        | product_ids::TX_RACING => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T300 RS",
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
        product_ids::T3PA | product_ids::T3PA_PRO => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T3PA",
            category: ThrustmasterDeviceCategory::Pedals,
            supports_ffb: false,
        },
        product_ids::T_LCM | product_ids::T_LCM_PRO => ThrustmasterDeviceIdentity {
            product_id,
            name: "Thrustmaster T-LCM",
            category: ThrustmasterDeviceCategory::Pedals,
            supports_ffb: false,
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
    fn test_identify_t_lcm() {
        let identity = identify_device(product_ids::T_LCM);
        assert_eq!(identity.category, ThrustmasterDeviceCategory::Pedals);
        assert!(!identity.supports_ffb);
    }

    #[test]
    fn test_is_wheel_product() {
        assert!(is_wheel_product(product_ids::TS_XW));
        assert!(is_wheel_product(product_ids::T300_RS));
        assert!(is_wheel_product(product_ids::T818));
        assert!(!is_wheel_product(product_ids::T_LCM));
    }

    #[test]
    fn test_is_pedal_product() {
        assert!(is_pedal_product(product_ids::T_LCM));
        assert!(is_pedal_product(product_ids::T3PA));
        assert!(!is_pedal_product(product_ids::TS_XW));
    }

    #[test]
    fn test_model_from_pid() {
        assert_eq!(Model::from_product_id(product_ids::TS_XW), Model::TSXW);
        assert_eq!(Model::from_product_id(product_ids::T818), Model::T818);
        assert_eq!(Model::from_product_id(product_ids::T_LCM), Model::TLCM);
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
        assert_eq!(Model::T150.max_rotation_deg(), 900);
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
}
