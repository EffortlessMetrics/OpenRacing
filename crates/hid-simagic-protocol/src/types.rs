//! Simagic device types: models, categories, pedal axes, and device identity.

#![deny(static_mut_refs)]

use crate::ids::product_ids;

/// High-level category for Simagic USB products.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimagicDeviceCategory {
    Wheelbase,
    Pedals,
    Shifter,
    Handbrake,
    Rim,
    Unknown,
}

/// Identity metadata for a Simagic product ID.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SimagicDeviceIdentity {
    pub product_id: u16,
    pub name: &'static str,
    pub category: SimagicDeviceCategory,
    pub supports_ffb: bool,
    pub max_torque_nm: Option<f32>,
}

/// Identify a Simagic product.
pub fn identify_device(product_id: u16) -> SimagicDeviceIdentity {
    match product_id {
        product_ids::EVO_SPORT => SimagicDeviceIdentity {
            product_id,
            name: "Simagic EVO Sport",
            category: SimagicDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(9.0),
        },
        product_ids::EVO => SimagicDeviceIdentity {
            product_id,
            name: "Simagic EVO",
            category: SimagicDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(12.0),
        },
        product_ids::EVO_PRO => SimagicDeviceIdentity {
            product_id,
            name: "Simagic EVO Pro",
            category: SimagicDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(18.0),
        },
        product_ids::ALPHA_EVO => SimagicDeviceIdentity {
            product_id,
            name: "Simagic Alpha EVO",
            category: SimagicDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(15.0),
        },
        product_ids::NEO => SimagicDeviceIdentity {
            product_id,
            name: "Simagic Neo",
            category: SimagicDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(10.0),
        },
        product_ids::NEO_MINI => SimagicDeviceIdentity {
            product_id,
            name: "Simagic Neo Mini",
            category: SimagicDeviceCategory::Wheelbase,
            supports_ffb: true,
            max_torque_nm: Some(7.0),
        },
        product_ids::P1000_PEDALS | product_ids::P1000A_PEDALS => SimagicDeviceIdentity {
            product_id,
            name: "Simagic P1000 Pedals",
            category: SimagicDeviceCategory::Pedals,
            supports_ffb: false,
            max_torque_nm: None,
        },
        product_ids::P2000_PEDALS => SimagicDeviceIdentity {
            product_id,
            name: "Simagic P2000 Pedals",
            category: SimagicDeviceCategory::Pedals,
            supports_ffb: false,
            max_torque_nm: None,
        },
        product_ids::SHIFTER_H => SimagicDeviceIdentity {
            product_id,
            name: "Simagic H-Pattern Shifter",
            category: SimagicDeviceCategory::Shifter,
            supports_ffb: false,
            max_torque_nm: None,
        },
        product_ids::SHIFTER_SEQ => SimagicDeviceIdentity {
            product_id,
            name: "Simagic Sequential Shifter",
            category: SimagicDeviceCategory::Shifter,
            supports_ffb: false,
            max_torque_nm: None,
        },
        product_ids::HANDBRAKE => SimagicDeviceIdentity {
            product_id,
            name: "Simagic Handbrake",
            category: SimagicDeviceCategory::Handbrake,
            supports_ffb: false,
            max_torque_nm: None,
        },
        product_ids::RIM_WR1
        | product_ids::RIM_GT1
        | product_ids::RIM_GT_NEO
        | product_ids::RIM_FORMULA => SimagicDeviceIdentity {
            product_id,
            name: "Simagic Rim",
            category: SimagicDeviceCategory::Rim,
            supports_ffb: false,
            max_torque_nm: None,
        },
        _ => SimagicDeviceIdentity {
            product_id,
            name: "Simagic Unknown",
            category: SimagicDeviceCategory::Unknown,
            supports_ffb: false,
            max_torque_nm: None,
        },
    }
}

/// Return true when the product ID is a known Simagic wheelbase.
pub fn is_wheelbase_product(product_id: u16) -> bool {
    matches!(
        identify_device(product_id).category,
        SimagicDeviceCategory::Wheelbase
    )
}

/// Simagic device model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimagicModel {
    EvoSport,
    Evo,
    EvoPro,
    AlphaEvo,
    Neo,
    NeoMini,
    P1000,
    P2000,
    ShifterH,
    ShifterSeq,
    Handbrake,
    Unknown,
}

impl SimagicModel {
    pub fn from_pid(pid: u16) -> Self {
        match pid {
            product_ids::EVO_SPORT => Self::EvoSport,
            product_ids::EVO => Self::Evo,
            product_ids::EVO_PRO => Self::EvoPro,
            product_ids::ALPHA_EVO => Self::AlphaEvo,
            product_ids::NEO => Self::Neo,
            product_ids::NEO_MINI => Self::NeoMini,
            product_ids::P1000_PEDALS | product_ids::P1000A_PEDALS => Self::P1000,
            product_ids::P2000_PEDALS => Self::P2000,
            product_ids::SHIFTER_H => Self::ShifterH,
            product_ids::SHIFTER_SEQ => Self::ShifterSeq,
            product_ids::HANDBRAKE => Self::Handbrake,
            _ => Self::Unknown,
        }
    }

    pub fn max_torque_nm(&self) -> f32 {
        match self {
            Self::EvoPro => 18.0,
            Self::Evo | Self::AlphaEvo => 12.0,
            Self::EvoSport => 9.0,
            Self::Neo => 10.0,
            Self::NeoMini => 7.0,
            Self::P1000 | Self::P2000 | Self::ShifterH | Self::ShifterSeq | Self::Handbrake => 0.0,
            Self::Unknown => 9.0,
        }
    }
}

/// Raw pedal axis samples parsed from an input report.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimagicPedalAxesRaw {
    pub throttle: u16,
    pub brake: u16,
    pub clutch: u16,
    pub handbrake: u16,
}

/// Normalized pedal axis samples in the `[0.0, 1.0]` range.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct SimagicPedalAxes {
    pub throttle: f32,
    pub brake: f32,
    pub clutch: f32,
    pub handbrake: f32,
}

/// Shifter gear state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SimagicGear {
    #[default]
    Neutral,
    First,
    Second,
    Third,
    Fourth,
    Fifth,
    Sixth,
    Seventh,
    Eighth,
    Unknown,
}

impl SimagicGear {
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Neutral,
            1 => Self::First,
            2 => Self::Second,
            3 => Self::Third,
            4 => Self::Fourth,
            5 => Self::Fifth,
            6 => Self::Sixth,
            7 => Self::Seventh,
            8 => Self::Eighth,
            _ => Self::Unknown,
        }
    }
}

/// Shifter state for H-pattern shifters.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SimagicShifterState {
    pub gear: SimagicGear,
    pub clutch_in_range: bool,
    pub sequential_up_pressed: bool,
    pub sequential_down_pressed: bool,
}

/// Quick release system status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum QuickReleaseStatus {
    Attached,
    Detached,
    #[default]
    Unknown,
}

impl QuickReleaseStatus {
    pub fn from_raw(value: u8) -> Self {
        match value {
            0 => Self::Attached,
            1 => Self::Detached,
            _ => Self::Unknown,
        }
    }
}

/// FFB effect types supported by Simagic wheelbases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SimagicFfbEffectType {
    Constant,
    Spring,
    Damper,
    Friction,
    Sine,
    Square,
    Triangle,
}

impl SimagicFfbEffectType {
    pub fn report_id(&self) -> u8 {
        match self {
            Self::Constant => 0x11,
            Self::Spring => 0x12,
            Self::Damper => 0x13,
            Self::Friction => 0x14,
            Self::Sine => 0x15,
            Self::Square => 0x16,
            Self::Triangle => 0x17,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identify_device_known_pids() {
        let known_pids = [
            // EVO wheelbases (verified)
            0x0500u16, 0x0501, 0x0502, // Accessories (estimated PIDs)
            0x1001, 0x1002, 0x2001, 0x2002, 0x3001,
        ];

        for &pid in &known_pids {
            let identity = identify_device(pid);
            assert!(!identity.name.is_empty());
            match identity.category {
                SimagicDeviceCategory::Wheelbase => {
                    assert!(identity.supports_ffb);
                    assert!(identity.max_torque_nm.is_some());
                }
                SimagicDeviceCategory::Pedals
                | SimagicDeviceCategory::Shifter
                | SimagicDeviceCategory::Handbrake
                | SimagicDeviceCategory::Rim => {
                    assert!(!identity.supports_ffb);
                }
                SimagicDeviceCategory::Unknown => {}
            }
        }
    }

    #[test]
    fn test_is_wheelbase_product_known_wheelbases() {
        let wheelbase_pids = [0x0500u16, 0x0501, 0x0502];

        for &pid in &wheelbase_pids {
            assert!(is_wheelbase_product(pid));
        }
    }

    #[test]
    fn test_is_wheelbase_product_non_wheelbases() {
        let non_wheelbase_pids = [0x1001u16, 0x1002, 0x2001, 0x2002, 0x3001];

        for &pid in &non_wheelbase_pids {
            assert!(!is_wheelbase_product(pid));
        }
    }

    #[test]
    fn test_simagic_model_max_torque() {
        let models_and_torques = [
            (SimagicModel::EvoSport, 9.0),
            (SimagicModel::Evo, 12.0),
            (SimagicModel::EvoPro, 18.0),
            (SimagicModel::AlphaEvo, 12.0),
            (SimagicModel::Neo, 10.0),
            (SimagicModel::NeoMini, 7.0),
            (SimagicModel::P1000, 0.0),
            (SimagicModel::P2000, 0.0),
            (SimagicModel::ShifterH, 0.0),
            (SimagicModel::ShifterSeq, 0.0),
            (SimagicModel::Handbrake, 0.0),
            (SimagicModel::Unknown, 9.0),
        ];

        for (model, expected_torque) in models_and_torques {
            assert_eq!(model.max_torque_nm(), expected_torque);
        }
    }
}
