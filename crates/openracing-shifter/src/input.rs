//! Shifter input parsing

use super::{
    GearPosition, MAX_GEARS, NEUTRAL_GEAR, ShifterCapabilities, ShifterError, ShifterResult,
};

pub struct ShifterInput {
    pub gear: GearPosition,
    pub clutch: Option<u16>,
    pub paddle_up: bool,
    pub paddle_down: bool,
}

impl ShifterInput {
    pub fn parse_gamepad(data: &[u8]) -> ShifterResult<Self> {
        if data.len() < 4 {
            return Err(ShifterError::InvalidReport);
        }

        let gear_raw = data[2];
        let buttons = data[3];

        let gear = GearPosition::from_raw(gear_raw);
        let clutch = if data.len() >= 6 {
            Some(u16::from(data[4]) | (u16::from(data[5]) << 8))
        } else {
            None
        };

        let paddle_up = (buttons & 0x10) != 0;
        let paddle_down = (buttons & 0x20) != 0;

        Ok(Self {
            gear,
            clutch,
            paddle_up,
            paddle_down,
        })
    }

    pub fn from_sequential(up: bool, down: bool, current_gear: i32) -> Self {
        let gear = if up {
            GearPosition::new((current_gear + 1).min(MAX_GEARS as i32))
        } else if down {
            GearPosition::new((current_gear - 1).max(1))
        } else {
            GearPosition::new(current_gear)
        };

        Self {
            gear,
            clutch: None,
            paddle_up: up,
            paddle_down: down,
        }
    }

    pub fn gear(&self) -> i32 {
        self.gear.gear
    }

    pub fn is_shifting(&self) -> bool {
        self.paddle_up || self.paddle_down
    }
}

impl Default for ShifterInput {
    fn default() -> Self {
        Self {
            gear: GearPosition::neutral(),
            clutch: None,
            paddle_up: false,
            paddle_down: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_gamepad() {
        let data = vec![0x00, 0x00, 0x03, 0x00, 0x00, 0x00];
        let input = ShifterInput::parse_gamepad(&data).unwrap();

        assert_eq!(input.gear.gear, 3);
        assert!(!input.paddle_up);
        assert!(!input.paddle_down);
    }

    #[test]
    fn test_parse_gamepad_with_paddles() {
        let data = vec![0x00, 0x00, 0x04, 0x30, 0x00, 0x00];
        let input = ShifterInput::parse_gamepad(&data).unwrap();

        assert_eq!(input.gear.gear, 4);
        assert!(input.paddle_up);
        assert!(input.paddle_down);
    }

    #[test]
    fn test_sequential_shifter() {
        let input = ShifterInput::from_sequential(true, false, 3);

        assert_eq!(input.gear.gear, 4);
        assert!(input.paddle_up);
        assert!(!input.paddle_down);
    }

    #[test]
    fn test_sequential_shifter_down() {
        let input = ShifterInput::from_sequential(false, true, 4);

        assert_eq!(input.gear.gear, 3);
        assert!(!input.paddle_up);
        assert!(input.paddle_down);
    }

    #[test]
    fn test_sequential_shifter_bounds() {
        let input_max = ShifterInput::from_sequential(true, false, 8);
        assert_eq!(input_max.gear.gear, 8);

        let input_min = ShifterInput::from_sequential(false, true, 1);
        assert_eq!(input_min.gear.gear, 1);
    }

    #[test]
    fn test_is_shifting() {
        let idle = ShifterInput::default();
        assert!(!idle.is_shifting());

        let shifting = ShifterInput::from_sequential(true, false, 3);
        assert!(shifting.is_shifting());
    }

    #[test]
    fn test_invalid_report() {
        let data = vec![0x00];
        let result = ShifterInput::parse_gamepad(&data);
        assert!(matches!(result, Err(ShifterError::InvalidReport)));
    }
}
