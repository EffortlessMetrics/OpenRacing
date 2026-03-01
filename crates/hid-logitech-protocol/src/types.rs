//! Logitech device model classification.

#![deny(static_mut_refs)]

use crate::ids::product_ids;

/// Logitech wheel model classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogitechModel {
    /// WingMan Formula Force / Formula Force GP (~0.5 Nm, 180°, gear-driven).
    ///
    /// Kernel `lg4ff_devices[]`: WINGMAN_FFG (0xC293) max_range=180.
    /// WFF (0xC291, from oversteer) is the same hardware generation.
    WingManFormulaForce,
    /// MOMO Racing / MOMO Force wheel (2.2 Nm, 270°, gear-driven).
    MOMO,
    /// Driving Force / Formula EX (2.0 Nm, 270°, gear-driven).
    ///
    /// This PID (0xC294) is also reported by higher wheels (G25/G27/DFGT/G29)
    /// when running in DF-EX compatibility mode. If a mode-switch succeeds,
    /// the device re-enumerates with its native PID.
    DrivingForceEX,
    /// Driving Force Pro (900°, belt-driven).
    DrivingForcePro,
    /// Driving Force GT (900°, belt-driven, shift LEDs).
    DrivingForceGT,
    /// Speed Force Wireless (Wii racing wheel, 270°).
    SpeedForceWireless,
    /// Vibration Wheel (basic rumble, uses `lgff` not `lg4ff`, no range control).
    VibrationWheel,
    /// G25 racing wheel (2.5 Nm, 900°).
    G25,
    /// G27 racing wheel (2.5 Nm, 900°).
    G27,
    /// G29 racing wheel (2.2 Nm, 900°).
    G29,
    /// G920 racing wheel (2.2 Nm, 900°).
    G920,
    /// G923 racing wheel with TrueForce (2.2 Nm, 900°).
    G923,
    /// G PRO direct-drive racing wheel (11 Nm, 1080°; PS and Xbox/PC variants).
    GPro,
    /// Unknown or future Logitech wheel.
    Unknown,
}

impl LogitechModel {
    /// Classify a device by its product ID.
    pub fn from_product_id(product_id: u16) -> Self {
        match product_id {
            product_ids::MOMO | product_ids::MOMO_2 => Self::MOMO,
            product_ids::DRIVING_FORCE_EX => Self::DrivingForceEX,
            product_ids::DRIVING_FORCE_PRO => Self::DrivingForcePro,
            product_ids::DRIVING_FORCE_GT => Self::DrivingForceGT,
            product_ids::SPEED_FORCE_WIRELESS => Self::SpeedForceWireless,
            product_ids::WINGMAN_FORMULA_FORCE_GP
            | product_ids::WINGMAN_FORMULA_FORCE => Self::WingManFormulaForce,
            product_ids::VIBRATION_WHEEL => Self::VibrationWheel,
            product_ids::G25 => Self::G25,
            product_ids::G27 => Self::G27,
            product_ids::G29_PS => Self::G29,
            product_ids::G920 => Self::G920,
            product_ids::G923 | product_ids::G923_XBOX | product_ids::G923_PS => Self::G923,
            product_ids::G_PRO | product_ids::G_PRO_XBOX => Self::GPro,
            _ => Self::Unknown,
        }
    }

    /// Maximum continuous torque in Newton-meters for this model.
    ///
    /// These are manufacturer-specified peak torque values from Logitech
    /// product data sheets. They are **not** present in any open-source
    /// driver (drivers operate in dimensionless force units). Values are
    /// used here to normalize physical torque requests to the device's
    /// ±10000 magnitude range.
    pub fn max_torque_nm(self) -> f32 {
        match self {
            Self::WingManFormulaForce => 0.5,
            Self::MOMO
            | Self::DrivingForceEX
            | Self::DrivingForcePro
            | Self::DrivingForceGT
            | Self::SpeedForceWireless => 2.0,
            Self::VibrationWheel => 0.5,
            Self::G25 | Self::G27 => 2.5,
            Self::G29 | Self::G920 | Self::G923 => 2.2,
            Self::GPro => 11.0,
            Self::Unknown => 2.0,
        }
    }

    /// Maximum wheel rotation in degrees.
    ///
    /// Source: `lg4ff_devices[]` in kernel `hid-lg4ff.c`:
    /// - WingMan FFG (0xC293): 40-180° (gear-driven, ~year 2000)
    /// - WingMan FG (0xC20E): 40-180° (no FFB — not in our enum)
    /// - MOMO (0xC295), MOMO2 (0xCA03): 40-270°
    /// - DF/EX (0xC294): 40-270° ("WHEEL" in kernel)
    /// - SFW/WiiWheel (0xC29C): 40-270°
    /// - Vibration Wheel (0xCA04): ~270° (uses `lgff`, NOT `lg4ff`)
    /// - DFP, G25, DFGT, G27, G29, G920, G923: 40-900°
    /// - G PRO: 1080° (Logitech product specifications)
    pub fn max_rotation_deg(self) -> u16 {
        match self {
            Self::WingManFormulaForce => 180,
            Self::MOMO | Self::DrivingForceEX | Self::SpeedForceWireless
            | Self::VibrationWheel => 270,
            Self::GPro => 1080,
            _ => 900,
        }
    }

    /// Whether this model supports TrueForce haptics.
    ///
    /// TrueForce is a proprietary Logitech high-frequency haptic feedback
    /// feature exclusive to the G923 (both PS and Xbox variants). No public
    /// protocol specification exists in any open-source driver as of this
    /// writing: `berarma/new-lg4ff` supports G923 PS standard FFB but does
    /// not implement TrueForce; the Linux kernel `hid-logitech-hidpp`
    /// driver supports G923 Xbox standard FFB (as `QUIRK_CLASS_G920`) but
    /// likewise has no TrueForce code; `cvuchener/libhidpp` and SDL have
    /// no G923/TrueForce support at all. TrueForce requires the proprietary
    /// Logitech G HUB software on Windows.
    pub fn supports_trueforce(self) -> bool {
        matches!(self, Self::G923)
    }

    /// Whether this model has hardware-level friction effect support.
    ///
    /// Source: `berarma/new-lg4ff` `LG4FF_CAP_FRICTION` flag.
    /// Only DFP, G25, DFGT, and G27 have native hardware friction.
    /// G29, G920, G923, G PRO need software-emulated friction.
    pub fn supports_hardware_friction(self) -> bool {
        matches!(
            self,
            Self::DrivingForcePro | Self::G25 | Self::DrivingForceGT | Self::G27
        )
    }

    /// Whether this model supports adjustable rotation range via HID commands.
    ///
    /// Source: `lg4ff_devices[]` in kernel `hid-lg4ff.c` — only devices with a
    /// non-NULL `set_range` function pointer can adjust range at runtime.
    /// DFP uses `lg4ff_set_range_dfp`, G25/G27/DFGT/G29 use `lg4ff_set_range_g25`.
    /// Older wheels (WingMan, MOMO, DF-EX, SFW, Vibration Wheel) have NULL
    /// and must be physically set.
    pub fn supports_range_command(self) -> bool {
        matches!(
            self,
            Self::DrivingForcePro
                | Self::G25
                | Self::DrivingForceGT
                | Self::G27
                | Self::G29
                | Self::G920
                | Self::G923
                | Self::GPro
        )
    }
}

/// Return `true` if the product ID corresponds to a known Logitech wheel.
pub fn is_wheel_product(product_id: u16) -> bool {
    matches!(
        product_id,
        product_ids::MOMO
            | product_ids::MOMO_2
            | product_ids::WINGMAN_FORMULA_FORCE_GP
            | product_ids::WINGMAN_FORMULA_FORCE
            | product_ids::VIBRATION_WHEEL
            | product_ids::DRIVING_FORCE_EX
            | product_ids::DRIVING_FORCE_PRO
            | product_ids::DRIVING_FORCE_GT
            | product_ids::SPEED_FORCE_WIRELESS
            | product_ids::G25
            | product_ids::G27
            | product_ids::G29_PS
            | product_ids::G920
            | product_ids::G923
            | product_ids::G923_XBOX
            | product_ids::G923_PS
            | product_ids::G_PRO
            | product_ids::G_PRO_XBOX
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_g920() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::G920);
        assert_eq!(model, LogitechModel::G920);
        assert!((model.max_torque_nm() - 2.2).abs() < 0.05);
        assert_eq!(model.max_rotation_deg(), 900);
        assert!(!model.supports_trueforce());
        Ok(())
    }

    #[test]
    fn test_model_g923_trueforce() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::G923_XBOX);
        assert_eq!(model, LogitechModel::G923);
        assert!(model.supports_trueforce());
        Ok(())
    }

    #[test]
    fn test_model_g923_ps_trueforce() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::G923_PS);
        assert_eq!(model, LogitechModel::G923);
        assert!(model.supports_trueforce());
        Ok(())
    }

    #[test]
    fn test_model_g_pro() -> Result<(), Box<dyn std::error::Error>> {
        let model_ps = LogitechModel::from_product_id(product_ids::G_PRO);
        let model_xbox = LogitechModel::from_product_id(product_ids::G_PRO_XBOX);
        assert_eq!(model_ps, LogitechModel::GPro);
        assert_eq!(model_xbox, LogitechModel::GPro);
        assert!((model_ps.max_torque_nm() - 11.0).abs() < 0.05);
        assert_eq!(model_ps.max_rotation_deg(), 1080);
        Ok(())
    }

    #[test]
    fn test_model_unknown() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(0xDEAD);
        assert_eq!(model, LogitechModel::Unknown);
        Ok(())
    }

    #[test]
    fn test_is_wheel_product() -> Result<(), Box<dyn std::error::Error>> {
        assert!(is_wheel_product(product_ids::G920));
        assert!(is_wheel_product(product_ids::G923_XBOX));
        assert!(is_wheel_product(product_ids::G923_PS));
        assert!(is_wheel_product(product_ids::G_PRO));
        assert!(is_wheel_product(product_ids::G_PRO_XBOX));
        assert!(is_wheel_product(product_ids::G29_PS));
        assert!(!is_wheel_product(0xFFFF));
        assert!(!is_wheel_product(0x0000));
        Ok(())
    }

    #[test]
    fn test_model_g27() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::G27);
        assert_eq!(model, LogitechModel::G27);
        assert!((model.max_torque_nm() - 2.5).abs() < 0.05);
        Ok(())
    }

    /// Verify that all known PIDs round-trip correctly through from_product_id → is_wheel_product.
    #[test]
    fn test_all_known_pids_are_wheels() -> Result<(), Box<dyn std::error::Error>> {
        let known_pids = [
            product_ids::MOMO,
            product_ids::MOMO_2,
            product_ids::WINGMAN_FORMULA_FORCE_GP,
            product_ids::WINGMAN_FORMULA_FORCE,
            product_ids::VIBRATION_WHEEL,
            product_ids::DRIVING_FORCE_EX,
            product_ids::DRIVING_FORCE_PRO,
            product_ids::DRIVING_FORCE_GT,
            product_ids::SPEED_FORCE_WIRELESS,
            product_ids::G25,
            product_ids::G27,
            product_ids::G29_PS,
            product_ids::G920,
            product_ids::G923,
            product_ids::G923_PS,
            product_ids::G923_XBOX,
            product_ids::G_PRO,
            product_ids::G_PRO_XBOX,
        ];
        for pid in known_pids {
            assert!(
                is_wheel_product(pid),
                "PID 0x{:04X} should be a known wheel product",
                pid
            );
            let model = LogitechModel::from_product_id(pid);
            assert_ne!(
                model,
                LogitechModel::Unknown,
                "PID 0x{:04X} should classify to a known model",
                pid
            );
        }
        Ok(())
    }

    /// Verify specific VID/PID constant values against authoritative sources
    /// (Linux kernel hid-ids.h; new-lg4ff driver; oversteer project).
    #[test]
    fn test_pid_constant_values() -> Result<(), Box<dyn std::error::Error>> {
        use crate::ids::{LOGITECH_VENDOR_ID, product_ids};
        assert_eq!(LOGITECH_VENDOR_ID, 0x046D, "Logitech VID");
        assert_eq!(
            product_ids::G25,
            0xC299,
            "G25 PID (kernel: USB_DEVICE_ID_LOGITECH_G25_WHEEL)"
        );
        assert_eq!(
            product_ids::G27,
            0xC29B,
            "G27 PID (kernel: USB_DEVICE_ID_LOGITECH_G27_WHEEL)"
        );
        assert_eq!(
            product_ids::G29_PS,
            0xC24F,
            "G29 PID (kernel: USB_DEVICE_ID_LOGITECH_G29_WHEEL)"
        );
        assert_eq!(
            product_ids::G920,
            0xC262,
            "G920 PID (kernel: USB_DEVICE_ID_LOGITECH_G920_WHEEL)"
        );
        assert_eq!(
            product_ids::G923,
            0xC266,
            "G923 native PID (new-lg4ff: USB_DEVICE_ID_LOGITECH_G923_WHEEL)"
        );
        assert_eq!(
            product_ids::G923_PS,
            0xC267,
            "G923 PS compat PID (new-lg4ff: USB_DEVICE_ID_LOGITECH_G923_PS_WHEEL)"
        );
        assert_eq!(
            product_ids::G923_XBOX,
            0xC26E,
            "G923 Xbox PID (kernel: USB_DEVICE_ID_LOGITECH_G923_XBOX_WHEEL)"
        );
        assert_eq!(
            product_ids::G_PRO,
            0xC268,
            "G PRO PS PID (oversteer: LG_GPRO_PS)"
        );
        assert_eq!(
            product_ids::G_PRO_XBOX,
            0xC272,
            "G PRO Xbox PID (oversteer: LG_GPRO_XBOX)"
        );
        Ok(())
    }

    /// Kill mutants: delete match arm WingManFormulaForce → 180 in max_rotation_deg.
    #[test]
    fn test_wingman_max_rotation_180() -> Result<(), Box<dyn std::error::Error>> {
        let model = LogitechModel::from_product_id(product_ids::WINGMAN_FORMULA_FORCE_GP);
        assert_eq!(model, LogitechModel::WingManFormulaForce);
        assert_eq!(
            model.max_rotation_deg(),
            180,
            "WingMan FFG must have 180° rotation"
        );
        Ok(())
    }

    /// Kill mutants: delete match arm MOMO|DrivingForceEX|SFW|VibrationWheel → 270.
    #[test]
    fn test_legacy_270_degree_models() -> Result<(), Box<dyn std::error::Error>> {
        let momo = LogitechModel::from_product_id(product_ids::MOMO);
        assert_eq!(
            momo.max_rotation_deg(),
            270,
            "MOMO must have 270° rotation"
        );

        let dfex = LogitechModel::from_product_id(product_ids::DRIVING_FORCE_EX);
        assert_eq!(
            dfex.max_rotation_deg(),
            270,
            "DrivingForceEX must have 270° rotation"
        );

        let sfw = LogitechModel::from_product_id(product_ids::SPEED_FORCE_WIRELESS);
        assert_eq!(
            sfw.max_rotation_deg(),
            270,
            "SpeedForceWireless must have 270° rotation"
        );

        let vw = LogitechModel::from_product_id(product_ids::VIBRATION_WHEEL);
        assert_eq!(
            vw.max_rotation_deg(),
            270,
            "VibrationWheel must have 270° rotation"
        );

        Ok(())
    }

    /// Verify 270° models differ from the default 900° wildcard.
    #[test]
    fn test_270_differs_from_default_900() -> Result<(), Box<dyn std::error::Error>> {
        let momo = LogitechModel::MOMO;
        let g29 = LogitechModel::G29;
        assert_ne!(
            momo.max_rotation_deg(),
            g29.max_rotation_deg(),
            "270° models must differ from 900° default"
        );
        Ok(())
    }
}
