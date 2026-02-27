//! Device types for racing wheel hardware abstraction
//!
//! This crate provides device type definitions for racing wheel hardware,
//! abstracted from specific vendor implementations.

#![deny(unsafe_op_in_unsafe_fn)]
#![deny(clippy::unwrap_used)]

/// Telemetry data from device
#[derive(Debug, Clone)]
pub struct TelemetryData {
    pub wheel_angle_deg: f32,
    pub wheel_speed_rad_s: f32,
    pub temperature_c: u8,
    pub fault_flags: u8,
    pub hands_on: bool,
}

/// Generic non-RT control-surface snapshot used by input pipeline and diagnostics.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeviceInputs {
    pub tick: u32,
    pub buttons: [u8; 16],
    pub hat: u8,
    pub steering: Option<u16>,
    pub throttle: Option<u16>,
    pub brake: Option<u16>,
    pub clutch_left: Option<u16>,
    pub clutch_right: Option<u16>,
    pub clutch_combined: Option<u16>,
    pub clutch_left_button: Option<bool>,
    pub clutch_right_button: Option<bool>,
    pub handbrake: Option<u16>,
    pub rotaries: [i16; 8],
}

impl DeviceInputs {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_buttons(mut self, buttons: [u8; 16]) -> Self {
        self.buttons = buttons;
        self
    }

    pub fn with_steering(mut self, steering: u16) -> Self {
        self.steering = Some(steering);
        self
    }

    pub fn with_pedals(mut self, throttle: u16, brake: u16, clutch: u16) -> Self {
        self.throttle = Some(throttle);
        self.brake = Some(brake);
        self.clutch_combined = Some(clutch);
        self
    }

    pub fn with_handbrake(mut self, handbrake: u16) -> Self {
        self.handbrake = Some(handbrake);
        self
    }

    pub fn with_hat(mut self, hat: u8) -> Self {
        self.hat = hat;
        self
    }

    pub fn with_rotaries(mut self, rotaries: [i16; 8]) -> Self {
        self.rotaries = rotaries;
        self
    }

    pub fn button(&self, index: usize) -> bool {
        if index < 16 {
            self.buttons[index / 8] & (1 << (index % 8)) != 0
        } else {
            false
        }
    }

    pub fn set_button(&mut self, index: usize, value: bool) {
        if index < 16 {
            if value {
                self.buttons[index / 8] |= 1 << (index % 8);
            } else {
                self.buttons[index / 8] &= !(1 << (index % 8));
            }
        }
    }

    pub fn rotary(&self, index: usize) -> i16 {
        self.rotaries.get(index).copied().unwrap_or(0)
    }

    pub fn hat_direction(&self) -> HatDirection {
        match self.hat {
            0 => HatDirection::Up,
            1 => HatDirection::UpRight,
            2 => HatDirection::Right,
            3 => HatDirection::DownRight,
            4 => HatDirection::Down,
            5 => HatDirection::DownLeft,
            6 => HatDirection::Left,
            7 => HatDirection::UpLeft,
            _ => HatDirection::Neutral,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HatDirection {
    Up,
    UpRight,
    Right,
    DownRight,
    Down,
    DownLeft,
    Left,
    UpLeft,
    #[default]
    Neutral,
}

#[cfg(feature = "proptest")]
mod proptest_shrinks {
    use super::*;
    use proptest::prelude::*;

    impl Arbitrary for DeviceInputs {
        type Parameters = ();
        type Strategy = BoxedStrategy<Self>;

        fn arbitrary_with(_args: Self::Parameters) -> Self::Strategy {
            (
                (
                    any::<u32>(),
                    any::<[u8; 16]>(),
                    any::<u8>(),
                    any::<Option<u16>>(),
                    any::<Option<u16>>(),
                    any::<Option<u16>>(),
                ),
                (
                    any::<Option<u16>>(),
                    any::<Option<u16>>(),
                    any::<Option<u16>>(),
                    any::<Option<bool>>(),
                    any::<Option<bool>>(),
                    any::<Option<u16>>(),
                    any::<[i16; 8]>(),
                ),
            )
                .prop_map(
                    |(
                        (tick, buttons, hat, steering, throttle, brake),
                        (
                            clutch_left,
                            clutch_right,
                            clutch_combined,
                            clutch_left_button,
                            clutch_right_button,
                            handbrake,
                            rotaries,
                        ),
                    )| {
                        Self {
                            tick,
                            buttons,
                            hat,
                            steering,
                            throttle,
                            brake,
                            clutch_left,
                            clutch_right,
                            clutch_combined,
                            clutch_left_button,
                            clutch_right_button,
                            handbrake,
                            rotaries,
                        }
                    },
                )
                .boxed()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_inputs_default() {
        let inputs = DeviceInputs::default();
        assert_eq!(inputs.tick, 0);
        assert_eq!(inputs.buttons, [0u8; 16]);
        assert_eq!(inputs.hat, 0);
        assert!(inputs.steering.is_none());
    }

    #[test]
    fn test_device_inputs_builder() {
        let inputs = DeviceInputs::new()
            .with_buttons([
                0xFF, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
                0x00, 0x00,
            ])
            .with_steering(32768)
            .with_pedals(1024, 2048, 512)
            .with_handbrake(0);

        assert!(inputs.button(0));
        assert!(inputs.button(1));
        assert!(inputs.button(2));
        assert!(inputs.button(3));
        assert!(inputs.button(4));
        assert!(inputs.button(5));
        assert!(inputs.button(6));
        assert!(inputs.button(7));
        assert_eq!(inputs.steering, Some(32768));
        assert_eq!(inputs.throttle, Some(1024));
        assert_eq!(inputs.brake, Some(2048));
        assert_eq!(inputs.clutch_combined, Some(512));
    }

    #[test]
    fn test_button_access() {
        let mut inputs = DeviceInputs::default();

        inputs.set_button(0, true);
        assert!(inputs.button(0));

        inputs.set_button(7, true);
        assert!(inputs.button(7));

        inputs.set_button(0, false);
        assert!(!inputs.button(0));
        assert!(inputs.button(7));

        inputs.set_button(15, true);
        assert!(inputs.button(15));
    }

    #[test]
    fn test_rotary_access() {
        let inputs = DeviceInputs::new().with_rotaries([1, 2, 3, 4, 5, 6, 7, 8]);

        assert_eq!(inputs.rotary(0), 1);
        assert_eq!(inputs.rotary(7), 8);
        assert_eq!(inputs.rotary(8), 0);
    }

    #[test]
    fn test_hat_direction() {
        let mut inputs = DeviceInputs::default();

        for dir in 0..8 {
            inputs.hat = dir;
            assert_ne!(inputs.hat_direction(), HatDirection::Neutral);
        }

        inputs.hat = 0xFF;
        assert_eq!(inputs.hat_direction(), HatDirection::Neutral);
    }

    #[test]
    fn test_clutch_pedal_separation() {
        let inputs = DeviceInputs {
            clutch_left: Some(100),
            clutch_right: Some(200),
            clutch_combined: Some(150),
            ..Default::default()
        };

        assert_eq!(inputs.clutch_left, Some(100));
        assert_eq!(inputs.clutch_right, Some(200));
        assert_eq!(inputs.clutch_combined, Some(150));
    }

    #[test]
    fn test_telemetry_data() {
        let telemetry = TelemetryData {
            wheel_angle_deg: 45.0,
            wheel_speed_rad_s: 10.0,
            temperature_c: 50,
            fault_flags: 0,
            hands_on: true,
        };

        assert_eq!(telemetry.wheel_angle_deg, 45.0);
        assert_eq!(telemetry.temperature_c, 50);
        assert!(telemetry.hands_on);
    }
}
