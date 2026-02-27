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
}
