//! Dashboard overlay data plugin — computes display-ready values.
//!
//! This plugin demonstrates the **read-telemetry** capability used to derive
//! dashboard information: current gear indicator, RPM bar percentage, speed
//! display, shift light status, and race flag state.
//!
//! A UI layer (out of scope for this example) would read [`DashboardData`]
//! and render it on-screen or on an external display.
//!
//! # Real-time safety
//!
//! * No heap allocations — [`DashboardData`] is a plain value type.
//! * All arithmetic is bounded.
//! * Suitable for both WASM (60–200 Hz) and native (1 kHz) execution.

use openracing_plugin_abi::TelemetryFrame;

/// Configuration for the dashboard overlay.
#[derive(Debug, Clone, Copy)]
pub struct DashboardConfig {
    /// Maximum engine RPM for the RPM bar.
    pub max_rpm: f32,
    /// RPM threshold (0.0–1.0 of max) at which the shift light activates.
    pub shift_threshold: f32,
    /// Maximum displayable speed in km/h.
    pub max_speed_kmh: f32,
    /// Wheel circumference in metres (used to convert rad/s → km/h).
    pub wheel_circumference_m: f32,
}

impl Default for DashboardConfig {
    fn default() -> Self {
        Self {
            max_rpm: 8000.0,
            shift_threshold: 0.9,
            max_speed_kmh: 350.0,
            // Typical racing tyre ≈ 2.0 m circumference.
            wheel_circumference_m: 2.0,
        }
    }
}

/// Computed dashboard data for a single frame.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DashboardData {
    /// Current gear as a display character ('R', 'N', '1'–'9').
    pub gear_char: char,
    /// RPM bar fill percentage (0.0–1.0).
    pub rpm_bar: f32,
    /// Vehicle speed in km/h.
    pub speed_kmh: f32,
    /// Whether the shift light should be on.
    pub shift_light: bool,
    /// Active race flag (if any).
    pub flag: RaceFlag,
    /// Whether a fault condition is active.
    pub fault_active: bool,
}

/// Simplified race-flag enum derived from the telemetry fault_flags field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RaceFlag {
    /// No flag.
    None,
    /// Green flag — track clear.
    Green,
    /// Yellow flag — caution.
    Yellow,
    /// Red flag — session stopped.
    Red,
    /// Chequered flag — session end.
    Chequered,
}

/// Dashboard overlay plugin.
#[derive(Debug)]
pub struct DashboardOverlayPlugin {
    config: DashboardConfig,
}

impl DashboardOverlayPlugin {
    /// Create a new dashboard plugin.
    #[must_use]
    pub fn new(config: DashboardConfig) -> Self {
        Self { config }
    }

    /// Compute dashboard data from telemetry and supplementary inputs.
    ///
    /// * `telemetry` — current ABI telemetry frame.
    /// * `rpm` — engine RPM (not in the ABI frame; typically from game telemetry).
    /// * `gear` — current gear (-1 = reverse, 0 = neutral, 1+ = forward).
    /// * `flag_bits` — race flag bitfield (bit 0 = green, 1 = yellow, 2 = red, 3 = chequered).
    #[must_use]
    pub fn compute(
        &self,
        telemetry: &TelemetryFrame,
        rpm: f32,
        gear: i8,
        flag_bits: u8,
    ) -> DashboardData {
        let gear_char = gear_to_char(gear);
        let rpm_bar = compute_rpm_bar(rpm, self.config.max_rpm);
        let speed_kmh = wheel_speed_to_kmh(
            telemetry.wheel_speed_rad_s,
            self.config.wheel_circumference_m,
        )
        .min(self.config.max_speed_kmh);
        let shift_light = rpm_bar >= self.config.shift_threshold;
        let flag = decode_flag(flag_bits);
        let fault_active = telemetry.has_faults();

        DashboardData {
            gear_char,
            rpm_bar,
            speed_kmh,
            shift_light,
            flag,
            fault_active,
        }
    }
}

/// Convert gear number to display character.
fn gear_to_char(gear: i8) -> char {
    match gear {
        -1 => 'R',
        0 => 'N',
        1..=9 => (b'0' + gear as u8) as char,
        _ => '?',
    }
}

/// Compute RPM bar fill (0.0–1.0).
fn compute_rpm_bar(rpm: f32, max_rpm: f32) -> f32 {
    if max_rpm <= 0.0 {
        return 0.0;
    }
    (rpm / max_rpm).clamp(0.0, 1.0)
}

/// Convert wheel angular velocity (rad/s) to km/h.
fn wheel_speed_to_kmh(rad_s: f32, circumference_m: f32) -> f32 {
    // v = ω * r, circumference = 2πr → r = C / (2π)
    // v_m_s = rad_s * C / (2π)
    // v_kmh = v_m_s * 3.6
    let v_m_s = rad_s.abs() * circumference_m / (2.0 * core::f32::consts::PI);
    v_m_s * 3.6
}

/// Decode flag bitfield into a [`RaceFlag`].
fn decode_flag(bits: u8) -> RaceFlag {
    // Priority: red > chequered > yellow > green.
    if bits & 0b0100 != 0 {
        RaceFlag::Red
    } else if bits & 0b1000 != 0 {
        RaceFlag::Chequered
    } else if bits & 0b0010 != 0 {
        RaceFlag::Yellow
    } else if bits & 0b0001 != 0 {
        RaceFlag::Green
    } else {
        RaceFlag::None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn default_telemetry(wheel_speed: f32) -> TelemetryFrame {
        TelemetryFrame {
            wheel_speed_rad_s: wheel_speed,
            ..TelemetryFrame::default()
        }
    }

    #[test]
    fn gear_display_values() {
        assert_eq!(gear_to_char(-1), 'R');
        assert_eq!(gear_to_char(0), 'N');
        assert_eq!(gear_to_char(1), '1');
        assert_eq!(gear_to_char(5), '5');
        assert_eq!(gear_to_char(9), '9');
        assert_eq!(gear_to_char(10), '?');
        assert_eq!(gear_to_char(-2), '?');
    }

    #[test]
    fn rpm_bar_boundaries() {
        assert!((compute_rpm_bar(0.0, 8000.0)).abs() < f32::EPSILON);
        assert!((compute_rpm_bar(8000.0, 8000.0) - 1.0).abs() < f32::EPSILON);
        assert!((compute_rpm_bar(4000.0, 8000.0) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn rpm_bar_clamps_overflow() {
        assert!((compute_rpm_bar(10000.0, 8000.0) - 1.0).abs() < f32::EPSILON);
        assert!((compute_rpm_bar(-100.0, 8000.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn rpm_bar_zero_max() {
        assert!((compute_rpm_bar(5000.0, 0.0)).abs() < f32::EPSILON);
        assert!((compute_rpm_bar(5000.0, -1.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn speed_conversion_positive() {
        // At 2π rad/s with 2 m circumference → v = 2π * 2/(2π) = 2 m/s = 7.2 km/h.
        let two_pi = 2.0 * core::f32::consts::PI;
        let kmh = wheel_speed_to_kmh(two_pi, 2.0);
        assert!((kmh - 7.2).abs() < 0.01);
    }

    #[test]
    fn speed_conversion_negative_speed() {
        let two_pi = 2.0 * core::f32::consts::PI;
        let kmh = wheel_speed_to_kmh(-two_pi, 2.0);
        assert!((kmh - 7.2).abs() < 0.01);
    }

    #[test]
    fn flag_priority() {
        assert_eq!(decode_flag(0b0000), RaceFlag::None);
        assert_eq!(decode_flag(0b0001), RaceFlag::Green);
        assert_eq!(decode_flag(0b0010), RaceFlag::Yellow);
        assert_eq!(decode_flag(0b0100), RaceFlag::Red);
        assert_eq!(decode_flag(0b1000), RaceFlag::Chequered);
        // Red takes priority over yellow.
        assert_eq!(decode_flag(0b0110), RaceFlag::Red);
        // Chequered takes priority over yellow but not red.
        assert_eq!(decode_flag(0b1010), RaceFlag::Chequered);
    }

    #[test]
    fn shift_light_activates_at_threshold() {
        let config = DashboardConfig {
            max_rpm: 8000.0,
            shift_threshold: 0.9,
            ..Default::default()
        };
        let plugin = DashboardOverlayPlugin::new(config);
        let telem = default_telemetry(0.0);

        let below = plugin.compute(&telem, 7000.0, 4, 0);
        assert!(!below.shift_light);

        let at = plugin.compute(&telem, 7200.0, 4, 0);
        assert!(at.shift_light);

        let above = plugin.compute(&telem, 7500.0, 4, 0);
        assert!(above.shift_light);
    }

    #[test]
    fn fault_active_from_telemetry() {
        let plugin = DashboardOverlayPlugin::new(DashboardConfig::default());
        let clean = default_telemetry(0.0);
        assert!(!plugin.compute(&clean, 0.0, 0, 0).fault_active);

        let faulty = TelemetryFrame {
            fault_flags: 0x01,
            ..TelemetryFrame::default()
        };
        assert!(plugin.compute(&faulty, 0.0, 0, 0).fault_active);
    }

    #[test]
    fn speed_clamped_to_max() {
        let config = DashboardConfig {
            max_speed_kmh: 300.0,
            ..Default::default()
        };
        let plugin = DashboardOverlayPlugin::new(config);
        // Very high wheel speed.
        let telem = default_telemetry(1000.0);
        let data = plugin.compute(&telem, 0.0, 0, 0);
        assert!(data.speed_kmh <= 300.0);
    }

    #[test]
    fn full_compute_smoke_test() {
        let plugin = DashboardOverlayPlugin::new(DashboardConfig::default());
        let telem = TelemetryFrame::with_values(1_000_000, 45.0, 50.0, 40.0, 0);
        let data = plugin.compute(&telem, 6000.0, 3, 0b0001);

        assert_eq!(data.gear_char, '3');
        assert!(data.rpm_bar > 0.0 && data.rpm_bar < 1.0);
        assert!(data.speed_kmh > 0.0);
        assert!(!data.shift_light);
        assert_eq!(data.flag, RaceFlag::Green);
        assert!(!data.fault_active);
    }
}
