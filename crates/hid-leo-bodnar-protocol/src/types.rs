//! Leo Bodnar device classification and input capabilities.

use crate::ids::{
    PID_BBI32, PID_BU0836_16BIT, PID_BU0836A, PID_BU0836X, PID_FFB_JOYSTICK, PID_SLI_M,
    PID_USB_JOYSTICK, PID_WHEEL_INTERFACE,
};

/// Leo Bodnar product family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeoBodnarDevice {
    /// USB Joystick – generic input-only joystick (PID `0x0001`).
    UsbJoystick,
    /// BU0836A – 12-bit joystick interface, 8 axes + 32 buttons
    /// (PID `0x000B`, estimated from community reports).
    Bu0836a,
    /// BBI-32 Button Box Interface – 32 buttons, input-only (PID `0x000C`).
    Bbi32,
    /// USB Sim Racing Wheel Interface – HID PID FFB wheel (PID `0x000E`).
    WheelInterface,
    /// FFB Joystick – direct drive force feedback joystick (PID `0x000F`).
    FfbJoystick,
    /// BU0836X – 12-bit joystick interface with push-in connectors,
    /// 8 axes + 32 buttons (PID `0x0030`, estimated from community reports).
    Bu0836x,
    /// BU0836 16-bit – 16-bit ADC joystick interface, 8 axes + 32 buttons
    /// (PID `0x0031`, estimated from community reports).
    Bu0836_16bit,
    /// SLI-Pro Shift Light Indicator – RPM/gear display device (PID `0x1301`,
    /// **community estimate**; see `ids::PID_SLI_M` doc comment).
    SlimShiftLight,
}

impl LeoBodnarDevice {
    /// Resolve a device variant from a USB product ID.
    ///
    /// Returns `None` for unrecognised product IDs.
    pub fn from_product_id(pid: u16) -> Option<Self> {
        match pid {
            PID_USB_JOYSTICK => Some(Self::UsbJoystick),
            PID_BU0836A => Some(Self::Bu0836a),
            PID_BBI32 => Some(Self::Bbi32),
            PID_WHEEL_INTERFACE => Some(Self::WheelInterface),
            PID_FFB_JOYSTICK => Some(Self::FfbJoystick),
            PID_BU0836X => Some(Self::Bu0836x),
            PID_BU0836_16BIT => Some(Self::Bu0836_16bit),
            PID_SLI_M => Some(Self::SlimShiftLight),
            _ => None,
        }
    }

    /// Maximum number of digital input channels (buttons) reported by this device.
    ///
    /// Returns `0` for output-only or display devices such as the SLI-M.
    pub fn max_input_channels(&self) -> u8 {
        match self {
            Self::UsbJoystick => 32,
            Self::Bu0836a => 32,
            Self::Bbi32 => 32,
            Self::WheelInterface => 32,
            Self::FfbJoystick => 32,
            Self::Bu0836x => 32,
            Self::Bu0836_16bit => 32,
            // SLI-M is primarily a shift light output device; no button inputs.
            Self::SlimShiftLight => 0,
        }
    }

    /// Returns `true` if the device supports HID PID force feedback.
    pub fn supports_ffb(&self) -> bool {
        matches!(self, Self::WheelInterface | Self::FfbJoystick)
    }

    /// Human-readable product name.
    pub fn name(&self) -> &'static str {
        match self {
            Self::UsbJoystick => "Leo Bodnar USB Joystick",
            Self::Bu0836a => "Leo Bodnar BU0836A",
            Self::Bbi32 => "Leo Bodnar BBI-32",
            Self::WheelInterface => "Leo Bodnar USB Sim Racing Wheel Interface",
            Self::FfbJoystick => "Leo Bodnar FFB Joystick",
            Self::Bu0836x => "Leo Bodnar BU0836X",
            Self::Bu0836_16bit => "Leo Bodnar BU0836 16-bit",
            Self::SlimShiftLight => "Leo Bodnar SLI-Pro",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::{
        PID_BBI32, PID_BU0836_16BIT, PID_BU0836A, PID_BU0836X, PID_FFB_JOYSTICK, PID_SLI_M,
        PID_USB_JOYSTICK, PID_WHEEL_INTERFACE,
    };

    #[test]
    fn from_pid_known() {
        assert_eq!(
            LeoBodnarDevice::from_product_id(PID_USB_JOYSTICK),
            Some(LeoBodnarDevice::UsbJoystick)
        );
        assert_eq!(
            LeoBodnarDevice::from_product_id(PID_BBI32),
            Some(LeoBodnarDevice::Bbi32)
        );
        assert_eq!(
            LeoBodnarDevice::from_product_id(PID_WHEEL_INTERFACE),
            Some(LeoBodnarDevice::WheelInterface)
        );
        assert_eq!(
            LeoBodnarDevice::from_product_id(PID_FFB_JOYSTICK),
            Some(LeoBodnarDevice::FfbJoystick)
        );
        assert_eq!(
            LeoBodnarDevice::from_product_id(PID_SLI_M),
            Some(LeoBodnarDevice::SlimShiftLight)
        );
        assert_eq!(
            LeoBodnarDevice::from_product_id(PID_BU0836A),
            Some(LeoBodnarDevice::Bu0836a)
        );
        assert_eq!(
            LeoBodnarDevice::from_product_id(PID_BU0836X),
            Some(LeoBodnarDevice::Bu0836x)
        );
        assert_eq!(
            LeoBodnarDevice::from_product_id(PID_BU0836_16BIT),
            Some(LeoBodnarDevice::Bu0836_16bit)
        );
    }

    #[test]
    fn from_pid_unknown_returns_none() {
        assert_eq!(LeoBodnarDevice::from_product_id(0xFFFF), None);
        assert_eq!(LeoBodnarDevice::from_product_id(0x0000), None);
        assert_eq!(LeoBodnarDevice::from_product_id(0x1234), None);
    }

    #[test]
    fn button_boxes_have_32_channels() {
        assert_eq!(LeoBodnarDevice::Bbi32.max_input_channels(), 32);
        assert_eq!(LeoBodnarDevice::Bu0836a.max_input_channels(), 32);
        assert_eq!(LeoBodnarDevice::Bu0836x.max_input_channels(), 32);
        assert_eq!(LeoBodnarDevice::Bu0836_16bit.max_input_channels(), 32);
    }

    #[test]
    fn shift_light_has_no_input_channels() {
        assert_eq!(LeoBodnarDevice::SlimShiftLight.max_input_channels(), 0);
    }

    #[test]
    fn ffb_support_correct() {
        assert!(LeoBodnarDevice::WheelInterface.supports_ffb());
        assert!(LeoBodnarDevice::FfbJoystick.supports_ffb());
        assert!(!LeoBodnarDevice::Bbi32.supports_ffb());
        assert!(!LeoBodnarDevice::UsbJoystick.supports_ffb());
        assert!(!LeoBodnarDevice::SlimShiftLight.supports_ffb());
    }

    #[test]
    fn names_are_non_empty() {
        let devices = [
            LeoBodnarDevice::UsbJoystick,
            LeoBodnarDevice::Bu0836a,
            LeoBodnarDevice::Bbi32,
            LeoBodnarDevice::WheelInterface,
            LeoBodnarDevice::FfbJoystick,
            LeoBodnarDevice::Bu0836x,
            LeoBodnarDevice::Bu0836_16bit,
            LeoBodnarDevice::SlimShiftLight,
        ];
        for device in &devices {
            assert!(
                !device.name().is_empty(),
                "name must not be empty for {device:?}"
            );
        }
    }
}
