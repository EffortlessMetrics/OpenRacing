//! Shifter type definitions

use serde::{Deserialize, Serialize};

pub const MAX_GEARS: usize = 8;
pub const NEUTRAL_GEAR: i32 = 0;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ShifterType {
    #[default]
    Sequential,
    HPattern,
    SequentialWithReverse,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct GearPosition {
    pub gear: i32,
    pub is_neutral: bool,
    pub is_reverse: bool,
}

impl GearPosition {
    pub fn new(gear: i32) -> Self {
        Self {
            gear,
            is_neutral: gear == NEUTRAL_GEAR,
            is_reverse: gear < NEUTRAL_GEAR,
        }
    }

    pub fn neutral() -> Self {
        Self::new(NEUTRAL_GEAR)
    }

    pub fn reverse() -> Self {
        Self::new(-1)
    }

    pub fn from_raw(raw: u8) -> Self {
        match raw {
            0 => Self::neutral(),
            1..=7 => Self::new(raw as i32),
            0xFF => Self::neutral(),
            _ => Self::neutral(),
        }
    }
}

impl Default for GearPosition {
    fn default() -> Self {
        Self::neutral()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ShifterCapabilities {
    pub shifter_type: ShifterType,
    pub max_gears: usize,
    pub has_clutch: bool,
    pub has_paddle_shifters: bool,
}

impl Default for ShifterCapabilities {
    fn default() -> Self {
        Self {
            shifter_type: ShifterType::Sequential,
            max_gears: MAX_GEARS,
            has_clutch: false,
            has_paddle_shifters: true,
        }
    }
}

impl ShifterCapabilities {
    pub fn sequential() -> Self {
        Self {
            shifter_type: ShifterType::Sequential,
            max_gears: MAX_GEARS,
            has_clutch: false,
            has_paddle_shifters: true,
        }
    }

    pub fn h_pattern() -> Self {
        Self {
            shifter_type: ShifterType::HPattern,
            max_gears: 6,
            has_clutch: true,
            has_paddle_shifters: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gear_position_neutral() {
        let gear = GearPosition::neutral();
        assert!(gear.is_neutral);
        assert!(!gear.is_reverse);
        assert_eq!(gear.gear, 0);
    }

    #[test]
    fn test_gear_position_reverse() {
        let gear = GearPosition::reverse();
        assert!(!gear.is_neutral);
        assert!(gear.is_reverse);
        assert_eq!(gear.gear, -1);
    }

    #[test]
    fn test_gear_position_from_raw() {
        assert!(GearPosition::from_raw(0).is_neutral);
        assert!(!GearPosition::from_raw(1).is_neutral);
        assert_eq!(GearPosition::from_raw(1).gear, 1);
        assert!(GearPosition::from_raw(0xFF).is_neutral);
    }

    #[test]
    fn test_gear_position_from_raw_all_valid_gears() {
        for gear in 1..=7u8 {
            let pos = GearPosition::from_raw(gear);
            assert_eq!(pos.gear, gear as i32);
            assert!(!pos.is_neutral);
            assert!(!pos.is_reverse);
        }
    }

    #[test]
    fn test_gear_position_from_raw_out_of_range() {
        for raw in [8u8, 10, 100, 200, 254] {
            let pos = GearPosition::from_raw(raw);
            assert!(pos.is_neutral, "raw {} should map to neutral", raw);
        }
    }

    #[test]
    fn test_gear_position_default() {
        let pos = GearPosition::default();
        assert!(pos.is_neutral);
        assert_eq!(pos.gear, 0);
    }

    #[test]
    fn test_gear_position_new_positive() {
        let pos = GearPosition::new(5);
        assert_eq!(pos.gear, 5);
        assert!(!pos.is_neutral);
        assert!(!pos.is_reverse);
    }

    #[test]
    fn test_gear_position_new_negative() {
        let pos = GearPosition::new(-2);
        assert_eq!(pos.gear, -2);
        assert!(!pos.is_neutral);
        assert!(pos.is_reverse);
    }

    #[test]
    fn test_shifter_capabilities_sequential() {
        let caps = ShifterCapabilities::sequential();
        assert_eq!(caps.shifter_type, ShifterType::Sequential);
        assert!(caps.has_paddle_shifters);
    }

    #[test]
    fn test_shifter_capabilities_h_pattern() {
        let caps = ShifterCapabilities::h_pattern();
        assert_eq!(caps.shifter_type, ShifterType::HPattern);
        assert!(caps.has_clutch);
    }

    #[test]
    fn test_shifter_capabilities_default() {
        let caps = ShifterCapabilities::default();
        assert_eq!(caps.shifter_type, ShifterType::Sequential);
        assert_eq!(caps.max_gears, MAX_GEARS);
    }

    #[test]
    fn test_shifter_type_default() {
        let st = ShifterType::default();
        assert_eq!(st, ShifterType::Sequential);
    }

    #[test]
    fn test_shifter_type_variants_distinct() {
        assert_ne!(ShifterType::Sequential, ShifterType::HPattern);
        assert_ne!(ShifterType::Sequential, ShifterType::SequentialWithReverse);
        assert_ne!(ShifterType::HPattern, ShifterType::SequentialWithReverse);
    }

    #[test]
    fn test_constants() {
        assert_eq!(MAX_GEARS, 8);
        assert_eq!(NEUTRAL_GEAR, 0);
    }
}
