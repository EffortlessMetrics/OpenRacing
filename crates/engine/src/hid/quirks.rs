//! Device quirks for handling hardware-specific FFB behavior
//!
//! Some devices have non-standard implementations of HID PID effects
//! that require workarounds. This module provides detection and
//! application of these quirks.

/// Device-specific quirks that affect FFB behavior
#[derive(Debug, Clone, Default)]
pub struct DeviceQuirks {
    /// Swap positive/negative coefficients for conditional effects
    /// (Spring, Damper, Friction, Inertia)
    ///
    /// Some devices (notably Moza) have inverted direction handling
    /// for conditional effects.
    pub fix_conditional_direction: bool,

    /// Device uses vendor-specific HID usage pages
    pub uses_vendor_usage_page: bool,

    /// Required USB polling interval (bInterval) in milliseconds
    pub required_b_interval: Option<u8>,

    /// Device requires initialization handshake before FFB works
    pub requires_init_handshake: bool,

    /// Device aggregates peripheral data in input reports
    pub aggregates_peripherals: bool,
}

impl DeviceQuirks {
    /// Get quirks for a specific device by VID/PID
    pub fn for_device(vendor_id: u16, product_id: u16) -> Self {
        match vendor_id {
            // Moza Racing
            0x346E => Self::moza_quirks(product_id),
            // Fanatec
            0x0EB7 => Self::fanatec_quirks(product_id),
            // Simagic
            0x0483 | 0x16D0 => Self::simagic_quirks(product_id),
            // Default - no quirks
            _ => Self::default(),
        }
    }

    /// Moza-specific quirks
    fn moza_quirks(product_id: u16) -> Self {
        let is_v2 = (product_id & 0x0010) != 0;
        let is_pedals = product_id == 0x0003;

        Self {
            // All Moza wheelbases have inverted conditional direction
            fix_conditional_direction: !is_pedals,
            uses_vendor_usage_page: true,
            required_b_interval: Some(1), // 1ms for 1kHz
            requires_init_handshake: !is_pedals,
            // V2 wheelbases aggregate peripheral data
            aggregates_peripherals: is_v2 && !is_pedals,
        }
    }

    /// Fanatec-specific quirks
    fn fanatec_quirks(_product_id: u16) -> Self {
        Self {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: Some(1),
            requires_init_handshake: false,
            aggregates_peripherals: false,
        }
    }

    /// Simagic-specific quirks
    fn simagic_quirks(_product_id: u16) -> Self {
        Self {
            fix_conditional_direction: false,
            uses_vendor_usage_page: false,
            required_b_interval: Some(1),
            requires_init_handshake: false,
            aggregates_peripherals: false,
        }
    }

    /// Check if any quirks are active
    pub fn has_quirks(&self) -> bool {
        self.fix_conditional_direction
            || self.uses_vendor_usage_page
            || self.required_b_interval.is_some()
            || self.requires_init_handshake
            || self.aggregates_peripherals
    }
}

/// Apply conditional direction fix to effect coefficients
///
/// For devices with `fix_conditional_direction` quirk, swaps the
/// positive and negative coefficients for Spring, Damper, Friction,
/// and Inertia effects.
#[derive(Debug, Clone, Copy)]
pub struct ConditionalCoefficients {
    pub positive_coefficient: i16,
    pub negative_coefficient: i16,
    pub positive_saturation: u16,
    pub negative_saturation: u16,
    pub dead_band: u16,
    pub center: i16,
}

impl ConditionalCoefficients {
    /// Apply direction fix if quirk is active
    pub fn apply_direction_fix(&self, fix_direction: bool) -> Self {
        if fix_direction {
            Self {
                positive_coefficient: self.negative_coefficient,
                negative_coefficient: self.positive_coefficient,
                positive_saturation: self.negative_saturation,
                negative_saturation: self.positive_saturation,
                dead_band: self.dead_band,
                center: self.center,
            }
        } else {
            *self
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_moza_quirks_detection() {
        // V1 wheelbases
        let r3_quirks = DeviceQuirks::for_device(0x346E, 0x0005);
        assert!(r3_quirks.fix_conditional_direction);
        assert!(r3_quirks.requires_init_handshake);
        assert!(!r3_quirks.aggregates_peripherals);

        // V2 wheelbases
        let r3_v2_quirks = DeviceQuirks::for_device(0x346E, 0x0015);
        assert!(r3_v2_quirks.fix_conditional_direction);
        assert!(r3_v2_quirks.aggregates_peripherals);

        // Pedals have no FFB quirks
        let pedal_quirks = DeviceQuirks::for_device(0x346E, 0x0003);
        assert!(!pedal_quirks.fix_conditional_direction);
        assert!(!pedal_quirks.requires_init_handshake);
    }

    #[test]
    fn test_unknown_device_no_quirks() {
        let quirks = DeviceQuirks::for_device(0x1234, 0x5678);
        assert!(!quirks.has_quirks());
    }

    #[test]
    fn test_conditional_direction_fix() {
        let coeffs = ConditionalCoefficients {
            positive_coefficient: 100,
            negative_coefficient: -50,
            positive_saturation: 1000,
            negative_saturation: 500,
            dead_band: 10,
            center: 0,
        };

        // Without fix
        let no_fix = coeffs.apply_direction_fix(false);
        assert_eq!(no_fix.positive_coefficient, 100);
        assert_eq!(no_fix.negative_coefficient, -50);

        // With fix - coefficients swapped
        let with_fix = coeffs.apply_direction_fix(true);
        assert_eq!(with_fix.positive_coefficient, -50);
        assert_eq!(with_fix.negative_coefficient, 100);
        assert_eq!(with_fix.positive_saturation, 500);
        assert_eq!(with_fix.negative_saturation, 1000);
    }
}
