//! Unified telemetry types for game and device data.
//!
//! This module provides the canonical telemetry types used across all OpenRacing components.
//! The `NormalizedTelemetry` struct combines data from all game adapters into a consistent format.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Canonical normalized telemetry data from racing games.
///
/// This struct provides a unified view of telemetry data that all game adapters
/// convert their game-specific data into. It combines fields from multiple sources
/// to support FFB calculation, LED displays, and diagnostics.
///
/// # Field Groups
/// - **Motion**: speed, steering_angle, throttle, brake
/// - **Engine**: rpm, gear, max_rpm
/// - **G-forces**: lateral_g, longitudinal_g, vertical_g
/// - **Tire slip**: slip_ratio, slip_angle per wheel
/// - **FFB**: ffb_scalar, ffb_torque_nm
/// - **Flags**: racing flags and assists status
/// - **Context**: car_id, track_id, session_id
/// - **Extended**: game-specific key-value data
///
/// # Example
/// ```
/// use racing_wheel_schemas::telemetry::NormalizedTelemetry;
/// use std::time::Instant;
///
/// let telemetry = NormalizedTelemetry::builder()
///     .speed_mps(45.0)
///     .rpm(6500.0)
///     .gear(4)
///     .steering_angle(0.15)
///     .throttle(0.8)
///     .brake(0.0)
///     .build();
/// ```
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalizedTelemetry {
    // === Motion Data ===
    /// Vehicle speed in meters per second.
    pub speed_mps: f32,

    /// Steering wheel angle in radians (positive = right, negative = left).
    pub steering_angle: f32,

    /// Throttle position (0.0 = released, 1.0 = fully pressed).
    pub throttle: f32,

    /// Brake position (0.0 = released, 1.0 = fully pressed).
    pub brake: f32,

    /// Clutch position (0.0 = released, 1.0 = fully engaged).
    #[serde(default)]
    pub clutch: f32,

    // === Engine Data ===
    /// Engine RPM (revolutions per minute).
    pub rpm: f32,

    /// Maximum engine RPM for redline calculation.
    #[serde(default)]
    pub max_rpm: f32,

    /// Current gear (-1 = reverse, 0 = neutral, 1+ = forward gears).
    pub gear: i8,

    /// Number of gears available.
    #[serde(default)]
    pub num_gears: u8,

    // === G-Forces ===
    /// Lateral acceleration in G-forces (positive = right).
    #[serde(default)]
    pub lateral_g: f32,

    /// Longitudinal acceleration in G-forces (positive = forward/acceleration).
    #[serde(default)]
    pub longitudinal_g: f32,

    /// Vertical acceleration in G-forces (positive = up).
    #[serde(default)]
    pub vertical_g: f32,

    // === Tire Slip ===
    /// Overall tire slip ratio (0.0 = no slip, 1.0 = full slip).
    #[serde(default)]
    pub slip_ratio: f32,

    /// Front-left tire slip angle in radians.
    #[serde(default)]
    pub slip_angle_fl: f32,

    /// Front-right tire slip angle in radians.
    #[serde(default)]
    pub slip_angle_fr: f32,

    /// Rear-left tire slip angle in radians.
    #[serde(default)]
    pub slip_angle_rl: f32,

    /// Rear-right tire slip angle in radians.
    #[serde(default)]
    pub slip_angle_rr: f32,

    /// Tire temperatures in Celsius (FL, FR, RL, RR).
    #[serde(default)]
    pub tire_temps_c: [u8; 4],

    /// Tire pressures in PSI (FL, FR, RL, RR).
    #[serde(default)]
    pub tire_pressures_psi: [f32; 4],

    // === Force Feedback ===
    /// Force feedback scalar value (-1.0 to 1.0).
    /// Represents the force feedback strength requested by the game.
    #[serde(default)]
    pub ffb_scalar: f32,

    /// Force feedback torque in Newton-meters (if available).
    #[serde(default)]
    pub ffb_torque_nm: f32,

    // === Racing Flags and Status ===
    /// Racing flags and assists status.
    #[serde(default)]
    pub flags: TelemetryFlags,

    // === Context ===
    /// Car identifier (if available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub car_id: Option<String>,

    /// Track identifier (if available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub track_id: Option<String>,

    /// Session identifier (if available).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,

    /// Position in race (1-based).
    #[serde(default)]
    pub position: u8,

    /// Lap number (0-based, current lap).
    #[serde(default)]
    pub lap: u16,

    /// Lap time in seconds for current lap.
    #[serde(default)]
    pub current_lap_time_s: f32,

    /// Best lap time in seconds.
    #[serde(default)]
    pub best_lap_time_s: f32,

    /// Last lap time in seconds.
    #[serde(default)]
    pub last_lap_time_s: f32,

    /// Time delta to car ahead in seconds (negative = ahead).
    #[serde(default)]
    pub delta_ahead_s: f32,

    /// Time delta to car behind in seconds (positive = behind).
    #[serde(default)]
    pub delta_behind_s: f32,

    // === Fuel and Engine ===
    /// Fuel level as percentage (0.0-1.0).
    #[serde(default)]
    pub fuel_percent: f32,

    /// Engine temperature in Celsius.
    #[serde(default)]
    pub engine_temp_c: f32,

    // === Extended Data ===
    /// Additional game-specific data that doesn't fit into standard fields.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub extended: HashMap<String, TelemetryValue>,

    // === Timing ===
    /// Timestamp when this telemetry sample was captured (monotonic).
    /// Skipped during serialization as Instant is not serializable.
    #[serde(skip, default = "default_timestamp")]
    pub timestamp: Instant,

    /// Sequence number for ordering.
    #[serde(default)]
    pub sequence: u64,
}

fn default_timestamp() -> Instant {
    Instant::now()
}

impl Default for NormalizedTelemetry {
    fn default() -> Self {
        Self {
            speed_mps: 0.0,
            steering_angle: 0.0,
            throttle: 0.0,
            brake: 0.0,
            clutch: 0.0,
            rpm: 0.0,
            max_rpm: 0.0,
            gear: 0,
            num_gears: 0,
            lateral_g: 0.0,
            longitudinal_g: 0.0,
            vertical_g: 0.0,
            slip_ratio: 0.0,
            slip_angle_fl: 0.0,
            slip_angle_fr: 0.0,
            slip_angle_rl: 0.0,
            slip_angle_rr: 0.0,
            tire_temps_c: [0; 4],
            tire_pressures_psi: [0.0; 4],
            ffb_scalar: 0.0,
            ffb_torque_nm: 0.0,
            flags: TelemetryFlags::default(),
            car_id: None,
            track_id: None,
            session_id: None,
            position: 0,
            lap: 0,
            current_lap_time_s: 0.0,
            best_lap_time_s: 0.0,
            last_lap_time_s: 0.0,
            delta_ahead_s: 0.0,
            delta_behind_s: 0.0,
            fuel_percent: 0.0,
            engine_temp_c: 0.0,
            extended: HashMap::new(),
            timestamp: Instant::now(),
            sequence: 0,
        }
    }
}

impl NormalizedTelemetry {
    /// Create a new telemetry instance with current timestamp.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a builder for constructing telemetry with validation.
    pub fn builder() -> NormalizedTelemetryBuilder {
        NormalizedTelemetryBuilder::new()
    }

    /// Create telemetry with a specific timestamp.
    pub fn with_timestamp(timestamp: Instant) -> Self {
        Self {
            timestamp,
            ..Default::default()
        }
    }

    /// Get speed in km/h.
    pub fn speed_kmh(&self) -> f32 {
        self.speed_mps * 3.6
    }

    /// Get speed in mph.
    pub fn speed_mph(&self) -> f32 {
        self.speed_mps * 2.237
    }

    /// Get the average slip angle across all tires.
    pub fn average_slip_angle(&self) -> f32 {
        (self.slip_angle_fl + self.slip_angle_fr + self.slip_angle_rl + self.slip_angle_rr) / 4.0
    }

    /// Get the front axle average slip angle.
    pub fn front_slip_angle(&self) -> f32 {
        (self.slip_angle_fl + self.slip_angle_fr) / 2.0
    }

    /// Get the rear axle average slip angle.
    pub fn rear_slip_angle(&self) -> f32 {
        (self.slip_angle_rl + self.slip_angle_rr) / 2.0
    }

    /// Check if the vehicle is stationary (speed below threshold).
    pub fn is_stationary(&self) -> bool {
        self.speed_mps < 0.5
    }

    /// Get total G-force magnitude.
    pub fn total_g(&self) -> f32 {
        (self.lateral_g * self.lateral_g + self.longitudinal_g * self.longitudinal_g).sqrt()
    }

    /// Check if telemetry has valid FFB data.
    pub fn has_ffb_data(&self) -> bool {
        self.ffb_scalar != 0.0 || self.ffb_torque_nm != 0.0
    }

    /// Check if telemetry has valid RPM data for LED display.
    pub fn has_rpm_data(&self) -> bool {
        self.rpm > 0.0 && self.max_rpm > 0.0
    }

    /// Get RPM as fraction of redline (0.0-1.0).
    pub fn rpm_fraction(&self) -> f32 {
        if self.max_rpm > 0.0 {
            (self.rpm / self.max_rpm).clamp(0.0, 1.0)
        } else {
            0.0
        }
    }

    /// Check if any racing flags are active.
    pub fn has_active_flags(&self) -> bool {
        self.flags.yellow_flag
            || self.flags.red_flag
            || self.flags.blue_flag
            || self.flags.checkered_flag
    }

    /// Get the time since this telemetry was captured.
    pub fn age(&self) -> Duration {
        self.timestamp.elapsed()
    }

    /// Get an extended telemetry value by key.
    pub fn get_extended(&self, key: &str) -> Option<&TelemetryValue> {
        self.extended.get(key)
    }

    /// Add an extended telemetry value.
    pub fn with_extended(mut self, key: impl Into<String>, value: TelemetryValue) -> Self {
        self.extended.insert(key.into(), value);
        self
    }

    /// Set the timestamp.
    pub fn with_timestamp_mut(mut self, timestamp: Instant) -> Self {
        self.timestamp = timestamp;
        self
    }

    /// Set the sequence number.
    pub fn with_sequence(mut self, sequence: u64) -> Self {
        self.sequence = sequence;
        self
    }

    // Backward-compatible builder methods (deprecated - use builder() instead)

    /// Set FFB scalar value (-1.0 to 1.0).
    #[deprecated(
        since = "0.2.0",
        note = "Use NormalizedTelemetry::builder().ffb_scalar() instead"
    )]
    pub fn with_ffb_scalar(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.ffb_scalar = value.clamp(-1.0, 1.0);
        }
        self
    }

    /// Set RPM value.
    #[deprecated(
        since = "0.2.0",
        note = "Use NormalizedTelemetry::builder().rpm() instead"
    )]
    pub fn with_rpm(mut self, value: f32) -> Self {
        if value >= 0.0 && value.is_finite() {
            self.rpm = value;
        }
        self
    }

    /// Set speed in meters per second.
    #[deprecated(
        since = "0.2.0",
        note = "Use NormalizedTelemetry::builder().speed_mps() instead"
    )]
    pub fn with_speed_ms(mut self, value: f32) -> Self {
        if value >= 0.0 && value.is_finite() {
            self.speed_mps = value;
        }
        self
    }

    /// Set slip ratio (0.0-1.0).
    #[deprecated(
        since = "0.2.0",
        note = "Use NormalizedTelemetry::builder().slip_ratio() instead"
    )]
    pub fn with_slip_ratio(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.slip_ratio = value.clamp(0.0, 1.0);
        }
        self
    }

    /// Set gear value.
    #[deprecated(
        since = "0.2.0",
        note = "Use NormalizedTelemetry::builder().gear() instead"
    )]
    pub fn with_gear(mut self, value: i8) -> Self {
        self.gear = value;
        self
    }

    /// Set car identifier.
    #[deprecated(
        since = "0.2.0",
        note = "Use NormalizedTelemetry::builder().car_id() instead"
    )]
    pub fn with_car_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        if !id.is_empty() {
            self.car_id = Some(id);
        }
        self
    }

    /// Set track identifier.
    #[deprecated(
        since = "0.2.0",
        note = "Use NormalizedTelemetry::builder().track_id() instead"
    )]
    pub fn with_track_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        if !id.is_empty() {
            self.track_id = Some(id);
        }
        self
    }

    /// Set racing flags.
    #[deprecated(
        since = "0.2.0",
        note = "Use NormalizedTelemetry::builder().flags() instead"
    )]
    pub fn with_flags(mut self, flags: TelemetryFlags) -> Self {
        self.flags = flags;
        self
    }

    /// Validate and clamp all fields to reasonable ranges.
    pub fn validated(self) -> Self {
        Self {
            speed_mps: if self.speed_mps.is_finite() {
                self.speed_mps.max(0.0)
            } else {
                0.0
            },
            steering_angle: if self.steering_angle.is_finite() {
                self.steering_angle
            } else {
                0.0
            },
            throttle: if self.throttle.is_finite() {
                self.throttle.clamp(0.0, 1.0)
            } else {
                0.0
            },
            brake: if self.brake.is_finite() {
                self.brake.clamp(0.0, 1.0)
            } else {
                0.0
            },
            clutch: if self.clutch.is_finite() {
                self.clutch.clamp(0.0, 1.0)
            } else {
                0.0
            },
            rpm: if self.rpm.is_finite() {
                self.rpm.max(0.0)
            } else {
                0.0
            },
            max_rpm: if self.max_rpm.is_finite() {
                self.max_rpm.max(0.0)
            } else {
                0.0
            },
            lateral_g: if self.lateral_g.is_finite() {
                self.lateral_g
            } else {
                0.0
            },
            longitudinal_g: if self.longitudinal_g.is_finite() {
                self.longitudinal_g
            } else {
                0.0
            },
            vertical_g: if self.vertical_g.is_finite() {
                self.vertical_g
            } else {
                0.0
            },
            slip_ratio: if self.slip_ratio.is_finite() {
                self.slip_ratio.clamp(0.0, 1.0)
            } else {
                0.0
            },
            slip_angle_fl: if self.slip_angle_fl.is_finite() {
                self.slip_angle_fl
            } else {
                0.0
            },
            slip_angle_fr: if self.slip_angle_fr.is_finite() {
                self.slip_angle_fr
            } else {
                0.0
            },
            slip_angle_rl: if self.slip_angle_rl.is_finite() {
                self.slip_angle_rl
            } else {
                0.0
            },
            slip_angle_rr: if self.slip_angle_rr.is_finite() {
                self.slip_angle_rr
            } else {
                0.0
            },
            ffb_scalar: if self.ffb_scalar.is_finite() {
                self.ffb_scalar.clamp(-1.0, 1.0)
            } else {
                0.0
            },
            ffb_torque_nm: if self.ffb_torque_nm.is_finite() {
                self.ffb_torque_nm
            } else {
                0.0
            },
            fuel_percent: if self.fuel_percent.is_finite() {
                self.fuel_percent.clamp(0.0, 1.0)
            } else {
                0.0
            },
            engine_temp_c: if self.engine_temp_c.is_finite() {
                self.engine_temp_c
            } else {
                0.0
            },
            ..self
        }
    }
}

/// Builder for constructing NormalizedTelemetry with validation.
#[derive(Debug, Default)]
pub struct NormalizedTelemetryBuilder {
    inner: NormalizedTelemetry,
}

impl NormalizedTelemetryBuilder {
    /// Create a new builder.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set speed in meters per second.
    pub fn speed_mps(mut self, value: f32) -> Self {
        if value.is_finite() && value >= 0.0 {
            self.inner.speed_mps = value;
        }
        self
    }

    /// Set steering angle in radians.
    pub fn steering_angle(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.steering_angle = value;
        }
        self
    }

    /// Set throttle position (0.0-1.0).
    pub fn throttle(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.throttle = value.clamp(0.0, 1.0);
        }
        self
    }

    /// Set brake position (0.0-1.0).
    pub fn brake(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.brake = value.clamp(0.0, 1.0);
        }
        self
    }

    /// Set clutch position (0.0-1.0).
    pub fn clutch(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.clutch = value.clamp(0.0, 1.0);
        }
        self
    }

    /// Set engine RPM.
    pub fn rpm(mut self, value: f32) -> Self {
        if value.is_finite() && value >= 0.0 {
            self.inner.rpm = value;
        }
        self
    }

    /// Set maximum RPM.
    pub fn max_rpm(mut self, value: f32) -> Self {
        if value.is_finite() && value >= 0.0 {
            self.inner.max_rpm = value;
        }
        self
    }

    /// Set current gear.
    pub fn gear(mut self, value: i8) -> Self {
        self.inner.gear = value;
        self
    }

    /// Set number of gears.
    pub fn num_gears(mut self, value: u8) -> Self {
        self.inner.num_gears = value;
        self
    }

    /// Set lateral G-force.
    pub fn lateral_g(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.lateral_g = value;
        }
        self
    }

    /// Set longitudinal G-force.
    pub fn longitudinal_g(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.longitudinal_g = value;
        }
        self
    }

    /// Set vertical G-force.
    pub fn vertical_g(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.vertical_g = value;
        }
        self
    }

    /// Set overall slip ratio (0.0-1.0).
    pub fn slip_ratio(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.slip_ratio = value.clamp(0.0, 1.0);
        }
        self
    }

    /// Set front-left slip angle.
    pub fn slip_angle_fl(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.slip_angle_fl = value;
        }
        self
    }

    /// Set front-right slip angle.
    pub fn slip_angle_fr(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.slip_angle_fr = value;
        }
        self
    }

    /// Set rear-left slip angle.
    pub fn slip_angle_rl(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.slip_angle_rl = value;
        }
        self
    }

    /// Set rear-right slip angle.
    pub fn slip_angle_rr(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.slip_angle_rr = value;
        }
        self
    }

    /// Set tire temperatures in Celsius.
    pub fn tire_temps_c(mut self, temps: [u8; 4]) -> Self {
        self.inner.tire_temps_c = temps;
        self
    }

    /// Set tire pressures in PSI.
    pub fn tire_pressures_psi(mut self, pressures: [f32; 4]) -> Self {
        self.inner.tire_pressures_psi = pressures;
        self
    }

    /// Set FFB scalar (-1.0 to 1.0).
    pub fn ffb_scalar(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.ffb_scalar = value.clamp(-1.0, 1.0);
        }
        self
    }

    /// Set FFB torque in Newton-meters.
    pub fn ffb_torque_nm(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.ffb_torque_nm = value;
        }
        self
    }

    /// Set racing flags.
    pub fn flags(mut self, flags: TelemetryFlags) -> Self {
        self.inner.flags = flags;
        self
    }

    /// Set car identifier.
    pub fn car_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        if !id.is_empty() {
            self.inner.car_id = Some(id);
        }
        self
    }

    /// Set track identifier.
    pub fn track_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        if !id.is_empty() {
            self.inner.track_id = Some(id);
        }
        self
    }

    /// Set session identifier.
    pub fn session_id(mut self, id: impl Into<String>) -> Self {
        let id = id.into();
        if !id.is_empty() {
            self.inner.session_id = Some(id);
        }
        self
    }

    /// Set race position.
    pub fn position(mut self, pos: u8) -> Self {
        self.inner.position = pos;
        self
    }

    /// Set current lap number.
    pub fn lap(mut self, lap: u16) -> Self {
        self.inner.lap = lap;
        self
    }

    /// Set current lap time in seconds.
    pub fn current_lap_time_s(mut self, time: f32) -> Self {
        if time.is_finite() && time >= 0.0 {
            self.inner.current_lap_time_s = time;
        }
        self
    }

    /// Set best lap time in seconds.
    pub fn best_lap_time_s(mut self, time: f32) -> Self {
        if time.is_finite() && time >= 0.0 {
            self.inner.best_lap_time_s = time;
        }
        self
    }

    /// Set last lap time in seconds.
    pub fn last_lap_time_s(mut self, time: f32) -> Self {
        if time.is_finite() && time >= 0.0 {
            self.inner.last_lap_time_s = time;
        }
        self
    }

    /// Set delta to car ahead in seconds.
    pub fn delta_ahead_s(mut self, delta: f32) -> Self {
        if delta.is_finite() {
            self.inner.delta_ahead_s = delta;
        }
        self
    }

    /// Set delta to car behind in seconds.
    pub fn delta_behind_s(mut self, delta: f32) -> Self {
        if delta.is_finite() {
            self.inner.delta_behind_s = delta;
        }
        self
    }

    /// Set fuel percentage (0.0-1.0).
    pub fn fuel_percent(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.fuel_percent = value.clamp(0.0, 1.0);
        }
        self
    }

    /// Set engine temperature in Celsius.
    pub fn engine_temp_c(mut self, value: f32) -> Self {
        if value.is_finite() {
            self.inner.engine_temp_c = value;
        }
        self
    }

    /// Set timestamp.
    pub fn timestamp(mut self, ts: Instant) -> Self {
        self.inner.timestamp = ts;
        self
    }

    /// Set sequence number.
    pub fn sequence(mut self, seq: u64) -> Self {
        self.inner.sequence = seq;
        self
    }

    /// Add an extended telemetry value.
    pub fn extended(mut self, key: impl Into<String>, value: TelemetryValue) -> Self {
        self.inner.extended.insert(key.into(), value);
        self
    }

    /// Build the telemetry instance.
    pub fn build(self) -> NormalizedTelemetry {
        self.inner
    }
}

/// Racing flags and status information.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TelemetryFlags {
    /// Yellow flag (caution).
    #[serde(default)]
    pub yellow_flag: bool,

    /// Red flag (session stopped).
    #[serde(default)]
    pub red_flag: bool,

    /// Blue flag (being lapped).
    #[serde(default)]
    pub blue_flag: bool,

    /// Checkered flag (race finished).
    #[serde(default)]
    pub checkered_flag: bool,

    /// Green flag (racing).
    #[serde(default = "default_true")]
    pub green_flag: bool,

    /// Pit limiter active.
    #[serde(default)]
    pub pit_limiter: bool,

    /// In pit lane.
    #[serde(default)]
    pub in_pits: bool,

    /// DRS (Drag Reduction System) available.
    #[serde(default)]
    pub drs_available: bool,

    /// DRS currently active.
    #[serde(default)]
    pub drs_active: bool,

    /// ERS (Energy Recovery System) available.
    #[serde(default)]
    pub ers_available: bool,

    /// ERS currently deploying.
    #[serde(default)]
    pub ers_active: bool,

    /// Launch control active.
    #[serde(default)]
    pub launch_control: bool,

    /// Traction control active.
    #[serde(default)]
    pub traction_control: bool,

    /// ABS active.
    #[serde(default)]
    pub abs_active: bool,

    /// Engine limiter active (rev limiter).
    #[serde(default)]
    pub engine_limiter: bool,

    /// Safety car deployed.
    #[serde(default)]
    pub safety_car: bool,

    /// Formation lap.
    #[serde(default)]
    pub formation_lap: bool,

    /// Session paused.
    #[serde(default)]
    pub session_paused: bool,
}

fn default_true() -> bool {
    true
}

impl Default for TelemetryFlags {
    fn default() -> Self {
        Self {
            yellow_flag: false,
            red_flag: false,
            blue_flag: false,
            checkered_flag: false,
            green_flag: true,
            pit_limiter: false,
            in_pits: false,
            drs_available: false,
            drs_active: false,
            ers_available: false,
            ers_active: false,
            launch_control: false,
            traction_control: false,
            abs_active: false,
            engine_limiter: false,
            safety_car: false,
            formation_lap: false,
            session_paused: false,
        }
    }
}

/// Extended telemetry value for game-specific data.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value")]
pub enum TelemetryValue {
    /// Floating-point value.
    Float(f32),
    /// Integer value.
    Integer(i32),
    /// Boolean value.
    Boolean(bool),
    /// String value.
    String(String),
}

/// Serializable version of NormalizedTelemetry for recording/replay.
///
/// Since `Instant` cannot be serialized, this struct uses a relative
/// timestamp in nanoseconds from an epoch for serialization purposes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetrySnapshot {
    /// Relative timestamp in nanoseconds.
    pub timestamp_ns: u64,

    /// Vehicle speed in meters per second.
    pub speed_mps: f32,

    /// Steering wheel angle in radians.
    pub steering_angle: f32,

    /// Throttle position (0.0-1.0).
    pub throttle: f32,

    /// Brake position (0.0-1.0).
    pub brake: f32,

    /// Clutch position (0.0-1.0).
    #[serde(default)]
    pub clutch: f32,

    /// Engine RPM.
    pub rpm: f32,

    /// Maximum RPM.
    #[serde(default)]
    pub max_rpm: f32,

    /// Current gear.
    pub gear: i8,

    /// Number of gears.
    #[serde(default)]
    pub num_gears: u8,

    /// Lateral G-force.
    #[serde(default)]
    pub lateral_g: f32,

    /// Longitudinal G-force.
    #[serde(default)]
    pub longitudinal_g: f32,

    /// Vertical G-force.
    #[serde(default)]
    pub vertical_g: f32,

    /// Overall slip ratio.
    #[serde(default)]
    pub slip_ratio: f32,

    /// Front-left slip angle.
    #[serde(default)]
    pub slip_angle_fl: f32,

    /// Front-right slip angle.
    #[serde(default)]
    pub slip_angle_fr: f32,

    /// Rear-left slip angle.
    #[serde(default)]
    pub slip_angle_rl: f32,

    /// Rear-right slip angle.
    #[serde(default)]
    pub slip_angle_rr: f32,

    /// FFB scalar.
    #[serde(default)]
    pub ffb_scalar: f32,

    /// FFB torque in Nm.
    #[serde(default)]
    pub ffb_torque_nm: f32,

    /// Racing flags.
    #[serde(default)]
    pub flags: TelemetryFlags,

    /// Position in race.
    #[serde(default)]
    pub position: u8,

    /// Lap number.
    #[serde(default)]
    pub lap: u16,

    /// Current lap time in seconds.
    #[serde(default)]
    pub current_lap_time_s: f32,

    /// Fuel percentage.
    #[serde(default)]
    pub fuel_percent: f32,

    /// Sequence number.
    #[serde(default)]
    pub sequence: u64,
}

impl TelemetrySnapshot {
    /// Create a snapshot from NormalizedTelemetry using a reference epoch.
    pub fn from_telemetry(telemetry: &NormalizedTelemetry, epoch: Instant) -> Self {
        let timestamp_ns = telemetry
            .timestamp
            .saturating_duration_since(epoch)
            .as_nanos() as u64;

        Self {
            timestamp_ns,
            speed_mps: telemetry.speed_mps,
            steering_angle: telemetry.steering_angle,
            throttle: telemetry.throttle,
            brake: telemetry.brake,
            clutch: telemetry.clutch,
            rpm: telemetry.rpm,
            max_rpm: telemetry.max_rpm,
            gear: telemetry.gear,
            num_gears: telemetry.num_gears,
            lateral_g: telemetry.lateral_g,
            longitudinal_g: telemetry.longitudinal_g,
            vertical_g: telemetry.vertical_g,
            slip_ratio: telemetry.slip_ratio,
            slip_angle_fl: telemetry.slip_angle_fl,
            slip_angle_fr: telemetry.slip_angle_fr,
            slip_angle_rl: telemetry.slip_angle_rl,
            slip_angle_rr: telemetry.slip_angle_rr,
            ffb_scalar: telemetry.ffb_scalar,
            ffb_torque_nm: telemetry.ffb_torque_nm,
            flags: telemetry.flags.clone(),
            position: telemetry.position,
            lap: telemetry.lap,
            current_lap_time_s: telemetry.current_lap_time_s,
            fuel_percent: telemetry.fuel_percent,
            sequence: telemetry.sequence,
        }
    }

    /// Convert snapshot to NormalizedTelemetry with a reference epoch.
    pub fn to_telemetry(&self, epoch: Instant) -> NormalizedTelemetry {
        NormalizedTelemetry {
            timestamp: epoch + Duration::from_nanos(self.timestamp_ns),
            speed_mps: self.speed_mps,
            steering_angle: self.steering_angle,
            throttle: self.throttle,
            brake: self.brake,
            clutch: self.clutch,
            rpm: self.rpm,
            max_rpm: self.max_rpm,
            gear: self.gear,
            num_gears: self.num_gears,
            lateral_g: self.lateral_g,
            longitudinal_g: self.longitudinal_g,
            vertical_g: self.vertical_g,
            slip_ratio: self.slip_ratio,
            slip_angle_fl: self.slip_angle_fl,
            slip_angle_fr: self.slip_angle_fr,
            slip_angle_rl: self.slip_angle_rl,
            slip_angle_rr: self.slip_angle_rr,
            ffb_scalar: self.ffb_scalar,
            ffb_torque_nm: self.ffb_torque_nm,
            flags: self.flags.clone(),
            position: self.position,
            lap: self.lap,
            current_lap_time_s: self.current_lap_time_s,
            fuel_percent: self.fuel_percent,
            sequence: self.sequence,
            ..Default::default()
        }
    }
}

/// Telemetry frame with timing information for streaming.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFrame {
    /// Normalized telemetry data.
    pub data: NormalizedTelemetry,

    /// Timestamp when frame was received (monotonic, nanoseconds).
    pub timestamp_ns: u64,

    /// Sequence number for ordering.
    pub sequence: u64,

    /// Raw data size for diagnostics.
    pub raw_size: usize,
}

impl TelemetryFrame {
    /// Create a new telemetry frame.
    pub fn new(
        data: NormalizedTelemetry,
        timestamp_ns: u64,
        sequence: u64,
        raw_size: usize,
    ) -> Self {
        Self {
            data,
            timestamp_ns,
            sequence,
            raw_size,
        }
    }

    /// Create a frame from telemetry with current timestamp.
    pub fn from_telemetry(data: NormalizedTelemetry, sequence: u64, raw_size: usize) -> Self {
        let timestamp_ns = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);

        Self {
            data,
            timestamp_ns,
            sequence,
            raw_size,
        }
    }
}

/// Telemetry field coverage information for documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TelemetryFieldCoverage {
    /// Game identifier.
    pub game_id: String,
    /// Game version.
    pub game_version: String,
    /// Speed field supported.
    pub speed: bool,
    /// RPM field supported.
    pub rpm: bool,
    /// Gear field supported.
    pub gear: bool,
    /// Steering angle supported.
    pub steering_angle: bool,
    /// Throttle field supported.
    pub throttle: bool,
    /// Brake field supported.
    pub brake: bool,
    /// Lateral G-force supported.
    pub lateral_g: bool,
    /// Longitudinal G-force supported.
    pub longitudinal_g: bool,
    /// Slip ratio supported.
    pub slip_ratio: bool,
    /// Per-wheel slip angles supported.
    pub slip_angles: bool,
    /// FFB scalar supported.
    pub ffb_scalar: bool,
    /// FFB torque supported.
    pub ffb_torque: bool,
    /// Racing flags supported.
    pub flags: bool,
    /// Car ID supported.
    pub car_id: bool,
    /// Track ID supported.
    pub track_id: bool,
    /// Lap timing supported.
    pub lap_timing: bool,
    /// Fuel level supported.
    pub fuel: bool,
    /// Extended fields available.
    pub extended_fields: Vec<String>,
}

// Device telemetry types (existing functionality preserved)

/// Device telemetry data with explicit units and field documentation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct TelemetryData {
    /// Wheel angle in degrees.
    /// Range: -1800.0 to +1800.0 degrees for 5-turn wheels.
    pub wheel_angle_deg: f32,

    /// Wheel speed in radians per second.
    /// Positive values indicate clockwise rotation.
    pub wheel_speed_rad_s: f32,

    /// Temperature in degrees Celsius.
    /// Typical range: 20-80°C for normal operation.
    pub temperature_c: u8,

    /// Fault flags bitfield.
    /// Each bit represents a specific fault condition.
    pub fault_flags: u8,

    /// Hands on wheel detection.
    pub hands_on: bool,

    /// Timestamp in milliseconds since system start.
    pub timestamp: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    #[test]
    fn test_default_telemetry() -> TestResult {
        let telemetry = NormalizedTelemetry::default();

        assert_eq!(telemetry.speed_mps, 0.0);
        assert_eq!(telemetry.rpm, 0.0);
        assert_eq!(telemetry.gear, 0);
        assert_eq!(telemetry.steering_angle, 0.0);
        assert_eq!(telemetry.throttle, 0.0);
        assert_eq!(telemetry.brake, 0.0);
        assert_eq!(telemetry.lateral_g, 0.0);
        assert_eq!(telemetry.longitudinal_g, 0.0);
        assert!(telemetry.car_id.is_none());
        assert!(telemetry.track_id.is_none());
        Ok(())
    }

    #[test]
    fn test_builder_pattern() -> TestResult {
        let telemetry = NormalizedTelemetry::builder()
            .speed_mps(50.0)
            .rpm(6000.0)
            .gear(4)
            .steering_angle(0.1)
            .throttle(0.8)
            .brake(0.0)
            .lateral_g(0.5)
            .longitudinal_g(0.2)
            .ffb_scalar(0.5)
            .car_id("ferrari_488".to_string())
            .track_id("spa".to_string())
            .build();

        assert_eq!(telemetry.speed_mps, 50.0);
        assert_eq!(telemetry.rpm, 6000.0);
        assert_eq!(telemetry.gear, 4);
        assert_eq!(telemetry.steering_angle, 0.1);
        assert_eq!(telemetry.throttle, 0.8);
        assert_eq!(telemetry.brake, 0.0);
        assert_eq!(telemetry.lateral_g, 0.5);
        assert_eq!(telemetry.longitudinal_g, 0.2);
        assert_eq!(telemetry.ffb_scalar, 0.5);
        assert_eq!(telemetry.car_id, Some("ferrari_488".to_string()));
        assert_eq!(telemetry.track_id, Some("spa".to_string()));
        Ok(())
    }

    #[test]
    fn test_speed_conversions() -> TestResult {
        let telemetry = NormalizedTelemetry::builder().speed_mps(27.78).build();

        let speed_kmh = telemetry.speed_kmh();
        let speed_mph = telemetry.speed_mph();

        assert!((speed_kmh - 100.0).abs() < 0.1);
        assert!((speed_mph - 62.14).abs() < 0.1);
        Ok(())
    }

    #[test]
    fn test_slip_angle_calculations() -> TestResult {
        let telemetry = NormalizedTelemetry::builder()
            .slip_angle_fl(0.02)
            .slip_angle_fr(0.04)
            .slip_angle_rl(0.06)
            .slip_angle_rr(0.08)
            .build();

        let avg = telemetry.average_slip_angle();
        let front = telemetry.front_slip_angle();
        let rear = telemetry.rear_slip_angle();

        assert!((avg - 0.05).abs() < 0.001);
        assert!((front - 0.03).abs() < 0.001);
        assert!((rear - 0.07).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_is_stationary() -> TestResult {
        let telemetry = NormalizedTelemetry::builder().speed_mps(0.4).build();
        assert!(telemetry.is_stationary());

        let telemetry = NormalizedTelemetry::builder().speed_mps(10.0).build();
        assert!(!telemetry.is_stationary());
        Ok(())
    }

    #[test]
    fn test_total_g() -> TestResult {
        let telemetry = NormalizedTelemetry::builder()
            .lateral_g(3.0)
            .longitudinal_g(4.0)
            .build();

        let total = telemetry.total_g();
        assert!((total - 5.0).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_rpm_fraction() -> TestResult {
        let telemetry = NormalizedTelemetry::builder()
            .rpm(6500.0)
            .max_rpm(8000.0)
            .build();

        assert!((telemetry.rpm_fraction() - 0.8125).abs() < 0.001);
        Ok(())
    }

    #[test]
    fn test_has_active_flags() -> TestResult {
        let telemetry = NormalizedTelemetry::default();
        assert!(!telemetry.has_active_flags());

        let telemetry = NormalizedTelemetry {
            flags: TelemetryFlags {
                yellow_flag: true,
                ..Default::default()
            },
            ..Default::default()
        };
        assert!(telemetry.has_active_flags());
        Ok(())
    }

    #[test]
    fn test_validation_clamps_values() -> TestResult {
        let telemetry = NormalizedTelemetry {
            throttle: 1.5,
            brake: -0.2,
            slip_ratio: 1.5,
            ffb_scalar: -2.0,
            speed_mps: -10.0,
            ..Default::default()
        };

        let validated = telemetry.validated();

        assert_eq!(validated.throttle, 1.0);
        assert_eq!(validated.brake, 0.0);
        assert_eq!(validated.slip_ratio, 1.0);
        assert_eq!(validated.ffb_scalar, -1.0);
        assert_eq!(validated.speed_mps, 0.0);
        Ok(())
    }

    #[test]
    fn test_nan_handling() -> TestResult {
        let telemetry = NormalizedTelemetry::builder()
            .speed_mps(f32::NAN)
            .throttle(f32::NAN)
            .rpm(f32::INFINITY)
            .ffb_scalar(f32::NEG_INFINITY)
            .build();

        assert_eq!(telemetry.speed_mps, 0.0);
        assert_eq!(telemetry.throttle, 0.0);
        assert_eq!(telemetry.rpm, 0.0);
        assert_eq!(telemetry.ffb_scalar, 0.0);
        Ok(())
    }

    #[test]
    fn test_snapshot_roundtrip() -> TestResult {
        let epoch = Instant::now();
        std::thread::sleep(std::time::Duration::from_millis(10));

        let telemetry = NormalizedTelemetry::builder()
            .speed_mps(50.0)
            .rpm(6000.0)
            .gear(4)
            .steering_angle(0.1)
            .throttle(0.8)
            .brake(0.0)
            .lateral_g(0.5)
            .longitudinal_g(0.2)
            .slip_angle_fl(0.01)
            .slip_angle_fr(0.02)
            .slip_angle_rl(0.03)
            .slip_angle_rr(0.04)
            .ffb_scalar(0.5)
            .sequence(42)
            .build();

        let snapshot = TelemetrySnapshot::from_telemetry(&telemetry, epoch);
        let restored = snapshot.to_telemetry(epoch);

        assert_eq!(restored.speed_mps, telemetry.speed_mps);
        assert_eq!(restored.rpm, telemetry.rpm);
        assert_eq!(restored.gear, telemetry.gear);
        assert_eq!(restored.steering_angle, telemetry.steering_angle);
        assert_eq!(restored.throttle, telemetry.throttle);
        assert_eq!(restored.brake, telemetry.brake);
        assert_eq!(restored.lateral_g, telemetry.lateral_g);
        assert_eq!(restored.longitudinal_g, telemetry.longitudinal_g);
        assert_eq!(restored.slip_angle_fl, telemetry.slip_angle_fl);
        assert_eq!(restored.slip_angle_fr, telemetry.slip_angle_fr);
        assert_eq!(restored.slip_angle_rl, telemetry.slip_angle_rl);
        assert_eq!(restored.slip_angle_rr, telemetry.slip_angle_rr);
        assert_eq!(restored.ffb_scalar, telemetry.ffb_scalar);
        assert_eq!(restored.sequence, telemetry.sequence);
        Ok(())
    }

    #[test]
    fn test_snapshot_serialization() -> TestResult {
        let snapshot = TelemetrySnapshot {
            timestamp_ns: 1000000,
            speed_mps: 50.0,
            steering_angle: 0.1,
            throttle: 0.8,
            brake: 0.0,
            clutch: 0.0,
            rpm: 6000.0,
            max_rpm: 8000.0,
            gear: 4,
            num_gears: 6,
            lateral_g: 0.5,
            longitudinal_g: 0.2,
            vertical_g: 0.0,
            slip_ratio: 0.1,
            slip_angle_fl: 0.01,
            slip_angle_fr: 0.02,
            slip_angle_rl: 0.03,
            slip_angle_rr: 0.04,
            ffb_scalar: 0.5,
            ffb_torque_nm: 5.0,
            flags: TelemetryFlags::default(),
            position: 1,
            lap: 5,
            current_lap_time_s: 82.5,
            fuel_percent: 0.75,
            sequence: 42,
        };

        let json = serde_json::to_string(&snapshot)?;
        let deserialized: TelemetrySnapshot = serde_json::from_str(&json)?;

        assert_eq!(deserialized.timestamp_ns, snapshot.timestamp_ns);
        assert_eq!(deserialized.speed_mps, snapshot.speed_mps);
        assert_eq!(deserialized.rpm, snapshot.rpm);
        assert_eq!(deserialized.gear, snapshot.gear);
        Ok(())
    }

    #[test]
    fn test_telemetry_frame_creation() -> TestResult {
        let telemetry = NormalizedTelemetry::builder().rpm(5000.0).build();

        let frame = TelemetryFrame::new(telemetry.clone(), 12345, 1, 64);

        assert_eq!(frame.data.rpm, 5000.0);
        assert_eq!(frame.timestamp_ns, 12345);
        assert_eq!(frame.sequence, 1);
        assert_eq!(frame.raw_size, 64);
        Ok(())
    }

    #[test]
    fn test_extended_values() -> TestResult {
        let telemetry = NormalizedTelemetry::builder()
            .extended("custom_field", TelemetryValue::Float(1.5))
            .extended("flag", TelemetryValue::Boolean(true))
            .build();

        assert_eq!(
            telemetry.get_extended("custom_field"),
            Some(&TelemetryValue::Float(1.5))
        );
        assert_eq!(
            telemetry.get_extended("flag"),
            Some(&TelemetryValue::Boolean(true))
        );
        assert_eq!(telemetry.get_extended("missing"), None);
        Ok(())
    }

    #[test]
    fn test_flags_default() -> TestResult {
        let flags = TelemetryFlags::default();

        assert!(!flags.yellow_flag);
        assert!(!flags.red_flag);
        assert!(!flags.blue_flag);
        assert!(!flags.checkered_flag);
        assert!(flags.green_flag);
        assert!(!flags.pit_limiter);
        assert!(!flags.in_pits);
        assert!(!flags.drs_available);
        assert!(!flags.drs_active);
        assert!(!flags.ers_available);
        assert!(!flags.ers_active);
        assert!(!flags.launch_control);
        assert!(!flags.traction_control);
        assert!(!flags.abs_active);
        Ok(())
    }

    #[test]
    fn test_telemetry_value_serialization() -> TestResult {
        let values = vec![
            TelemetryValue::Float(1.5),
            TelemetryValue::Integer(42),
            TelemetryValue::Boolean(true),
            TelemetryValue::String("test".to_string()),
        ];

        for value in values {
            let json = serde_json::to_string(&value)?;
            let deserialized: TelemetryValue = serde_json::from_str(&json)?;
            assert_eq!(value, deserialized);
        }
        Ok(())
    }

    #[test]
    fn test_telemetry_json_serialization() -> TestResult {
        let telemetry = NormalizedTelemetry::builder()
            .speed_mps(50.0)
            .rpm(6000.0)
            .gear(4)
            .car_id("test_car".to_string())
            .build();

        let json = serde_json::to_string(&telemetry)?;
        let deserialized: NormalizedTelemetry = serde_json::from_str(&json)?;

        assert_eq!(deserialized.speed_mps, 50.0);
        assert_eq!(deserialized.rpm, 6000.0);
        assert_eq!(deserialized.gear, 4);
        assert_eq!(deserialized.car_id, Some("test_car".to_string()));
        Ok(())
    }
}
