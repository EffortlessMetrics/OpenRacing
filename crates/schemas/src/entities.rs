//! Domain entities for the racing wheel software
//!
//! This module contains the core domain entities that represent the main
//! business objects in the system. These are pure domain objects with
//! no dependencies on infrastructure concerns.

use crate::domain::{
    CurvePoint, Degrees, DeviceId, DomainError, FrequencyHz, Gain, ProfileId, TorqueNm,
    validate_curve_monotonic,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Instant;

/// Device capabilities as reported by the hardware
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceCapabilities {
    /// Supports HID PID (Physical Interface Device) protocol
    pub supports_pid: bool,

    /// Supports raw torque commands at 1kHz
    pub supports_raw_torque_1khz: bool,

    /// Supports health/telemetry streaming
    pub supports_health_stream: bool,

    /// Supports LED bus for lighting control
    pub supports_led_bus: bool,

    /// Maximum torque the device can produce
    pub max_torque: TorqueNm,

    /// Encoder counts per revolution
    pub encoder_cpr: u16,

    /// Minimum report period in microseconds (typically 1000 for 1kHz)
    pub min_report_period_us: u16,
}

impl DeviceCapabilities {
    /// Create new device capabilities with validation
    pub fn new(
        supports_pid: bool,
        supports_raw_torque_1khz: bool,
        supports_health_stream: bool,
        supports_led_bus: bool,
        max_torque: TorqueNm,
        encoder_cpr: u16,
        min_report_period_us: u16,
    ) -> Self {
        Self {
            supports_pid,
            supports_raw_torque_1khz,
            supports_health_stream,
            supports_led_bus,
            max_torque,
            encoder_cpr,
            min_report_period_us,
        }
    }

    /// Check if device supports any form of force feedback
    pub fn supports_ffb(&self) -> bool {
        self.supports_pid || self.supports_raw_torque_1khz
    }

    /// Get the maximum update rate in Hz
    pub fn max_update_rate_hz(&self) -> f32 {
        1_000_000.0 / (self.min_report_period_us as f32)
    }
}

/// Device connection and operational state
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum DeviceState {
    /// Device is disconnected
    Disconnected = 0,

    /// Device is connected but not initialized
    Connected = 1,

    /// Device is active and ready for operation
    Active = 2,

    /// Device is in a fault state
    Faulted = 3,

    /// Device is in safe mode (limited torque)
    SafeMode = 4,
}

/// Device calibration data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CalibrationData {
    /// Center position in degrees (None if not calibrated)
    pub center_position: Option<f32>,

    /// Minimum position in degrees (None if not calibrated)
    pub min_position: Option<f32>,

    /// Maximum position in degrees (None if not calibrated)
    pub max_position: Option<f32>,

    /// Pedal calibration data (throttle, brake, clutch min/max values)
    pub pedal_ranges: Option<PedalCalibrationData>,

    /// Calibration timestamp
    pub calibrated_at: Option<String>,

    /// Calibration type that was performed
    pub calibration_type: CalibrationType,
}

/// Pedal calibration data
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PedalCalibrationData {
    /// Throttle pedal range (min, max)
    pub throttle: Option<(f32, f32)>,

    /// Brake pedal range (min, max)
    pub brake: Option<(f32, f32)>,

    /// Clutch pedal range (min, max)
    pub clutch: Option<(f32, f32)>,
}

/// Type of calibration performed
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CalibrationType {
    /// Center position only
    Center,

    /// Full range (min/max positions)
    Range,

    /// Pedal calibration
    Pedals,

    /// Complete calibration (center + range + pedals)
    Full,
}

impl CalibrationData {
    /// Create new calibration data
    pub fn new(calibration_type: CalibrationType) -> Self {
        Self {
            center_position: None,
            min_position: None,
            max_position: None,
            pedal_ranges: None,
            calibrated_at: Some(chrono::Utc::now().to_rfc3339()),
            calibration_type,
        }
    }

    /// Check if center position is calibrated
    pub fn has_center_calibration(&self) -> bool {
        self.center_position.is_some()
    }

    /// Check if range is calibrated
    pub fn has_range_calibration(&self) -> bool {
        self.min_position.is_some() && self.max_position.is_some()
    }

    /// Check if pedals are calibrated
    pub fn has_pedal_calibration(&self) -> bool {
        self.pedal_ranges.is_some()
    }

    /// Check if fully calibrated
    pub fn is_fully_calibrated(&self) -> bool {
        self.has_center_calibration() && self.has_range_calibration()
    }
}

/// Device type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i32)]
pub enum DeviceType {
    /// Other/unknown device type
    Other = 0,

    /// Wheel base (main force feedback unit)
    WheelBase = 1,

    /// Steering wheel (rim)
    SteeringWheel = 2,

    /// Pedal set
    Pedals = 3,

    /// Shifter (H-pattern or sequential)
    Shifter = 4,

    /// Handbrake
    Handbrake = 5,

    /// Button box or dashboard
    ButtonBox = 6,
}

/// Core device entity
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Device {
    /// Unique device identifier
    pub id: DeviceId,

    /// Human-readable device name
    pub name: String,

    /// Device type classification
    pub device_type: DeviceType,

    /// Device capabilities
    pub capabilities: DeviceCapabilities,

    /// Current device state
    pub state: DeviceState,

    /// Last seen timestamp (for connection monitoring)
    #[serde(skip)]
    pub last_seen: Option<Instant>,

    /// Current fault flags (bitfield)
    pub fault_flags: u8,

    /// Device firmware version (if available)
    pub firmware_version: Option<String>,

    /// Device serial number (if available)
    pub serial_number: Option<String>,
}

impl Device {
    /// Create a new device
    pub fn new(
        id: DeviceId,
        name: String,
        device_type: DeviceType,
        capabilities: DeviceCapabilities,
    ) -> Self {
        Self {
            id,
            name,
            device_type,
            capabilities,
            state: DeviceState::Connected,
            last_seen: Some(Instant::now()),
            fault_flags: 0,
            firmware_version: None,
            serial_number: None,
        }
    }

    /// Check if device is operational (connected and not faulted)
    pub fn is_operational(&self) -> bool {
        matches!(self.state, DeviceState::Active | DeviceState::SafeMode) && self.fault_flags == 0
    }

    /// Check if device has any faults
    pub fn has_faults(&self) -> bool {
        self.fault_flags != 0
    }

    /// Set device state
    pub fn set_state(&mut self, state: DeviceState) {
        self.state = state;
        self.last_seen = Some(Instant::now());
    }

    /// Set fault flags
    pub fn set_fault_flags(&mut self, flags: u8) {
        self.fault_flags = flags;
        if flags != 0 {
            self.state = DeviceState::Faulted;
        }
    }

    /// Clear all faults
    pub fn clear_faults(&mut self) {
        self.fault_flags = 0;
        if self.state == DeviceState::Faulted {
            self.state = DeviceState::Active;
        }
    }

    /// Update last seen timestamp
    pub fn update_last_seen(&mut self) {
        self.last_seen = Some(Instant::now());
    }
}

/// Notch filter configuration for eliminating specific frequencies
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NotchFilter {
    /// Center frequency to attenuate
    pub frequency: FrequencyHz,

    /// Quality factor (higher = narrower notch)
    pub q_factor: f32,

    /// Gain reduction in dB (typically negative)
    pub gain_db: f32,
}

impl NotchFilter {
    /// Create a new notch filter with validation
    pub fn new(frequency: FrequencyHz, q_factor: f32, gain_db: f32) -> Result<Self, DomainError> {
        if q_factor <= 0.0 || !q_factor.is_finite() {
            return Err(DomainError::InvalidFrequency(q_factor));
        }

        if !gain_db.is_finite() {
            return Err(DomainError::InvalidFrequency(gain_db));
        }

        Ok(Self {
            frequency,
            q_factor,
            gain_db,
        })
    }
}

/// Filter configuration for force feedback processing
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FilterConfig {
    /// Reconstruction filter level (0-8, higher = more smoothing)
    pub reconstruction: u8,

    /// Friction coefficient (0.0-1.0)
    pub friction: Gain,

    /// Damper coefficient (0.0-1.0)
    pub damper: Gain,

    /// Inertia coefficient (0.0-1.0)
    pub inertia: Gain,

    /// Notch filters for eliminating specific frequencies
    pub notch_filters: Vec<NotchFilter>,

    /// Slew rate limiter (0.0-1.0, higher = faster changes allowed)
    pub slew_rate: Gain,

    /// Force curve mapping points (must be monotonic)
    pub curve_points: Vec<CurvePoint>,

    /// Torque cap as fraction of maximum (0.0-1.0)
    pub torque_cap: Gain,

    /// Bumpstop model configuration
    pub bumpstop: BumpstopConfig,

    /// Hands-off detection configuration
    pub hands_off: HandsOffConfig,
}

/// Configuration for bumpstop model
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BumpstopConfig {
    /// Enable bumpstop model
    pub enabled: bool,

    /// Angle from center where bumpstop starts (degrees)
    pub start_angle: f32,

    /// Maximum angle before hard stop (degrees)
    pub max_angle: f32,

    /// Spring stiffness coefficient (0.0-1.0)
    pub stiffness: f32,

    /// Damping coefficient (0.0-1.0)
    pub damping: f32,
}

/// Configuration for hands-off detection
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HandsOffConfig {
    /// Enable hands-off detection
    pub enabled: bool,

    /// Torque threshold for hands-on detection (0.0-1.0)
    pub threshold: f32,

    /// Time before considering hands-off (seconds)
    pub timeout_seconds: f32,
}

impl Default for BumpstopConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            start_angle: 450.0, // Start at 450 degrees from center
            max_angle: 540.0,   // Hard stop at 540 degrees
            stiffness: 0.8,
            damping: 0.3,
        }
    }
}

impl Default for HandsOffConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.05,      // 5% torque threshold
            timeout_seconds: 5.0, // 5 second timeout
        }
    }
}

impl Default for FilterConfig {
    /// Create FilterConfig with stable 1kHz-safe defaults
    ///
    /// These defaults are designed to be stable at 1kHz update rates
    /// with no oscillation or instability.
    fn default() -> Self {
        Self {
            // Stable values - no reconstruction filtering
            reconstruction: 0,
            friction: match Gain::new(0.0) {
                Ok(v) => v,
                Err(e) => panic!("0.0 is a valid gain: {:?}", e),
            },
            damper: match Gain::new(0.0) {
                Ok(v) => v,
                Err(e) => panic!("0.0 is a valid gain: {:?}", e),
            },
            inertia: match Gain::new(0.0) {
                Ok(v) => v,
                Err(e) => panic!("0.0 is a valid gain: {:?}", e),
            },
            notch_filters: Vec::new(),
            slew_rate: match Gain::new(1.0) {
                Ok(v) => v,
                Err(e) => panic!("1.0 is a valid gain: {:?}", e),
            }, // No slew rate limiting
            curve_points: vec![
                match CurvePoint::new(0.0, 0.0) {
                    Ok(v) => v,
                    Err(e) => panic!("0.0, 0.0 is a valid curve point: {:?}", e),
                },
                match CurvePoint::new(1.0, 1.0) {
                    Ok(v) => v,
                    Err(e) => panic!("1.0, 1.0 is a valid curve point: {:?}", e),
                },
            ],
            torque_cap: match Gain::new(1.0) {
                Ok(v) => v,
                Err(e) => panic!("1.0 is a valid gain: {:?}", e),
            }, // No torque cap by default
            bumpstop: BumpstopConfig::default(),
            hands_off: HandsOffConfig::default(),
        }
    }
}

impl FilterConfig {
    /// Create a new filter configuration with validation
    pub fn new(
        reconstruction: u8,
        friction: Gain,
        damper: Gain,
        inertia: Gain,
        notch_filters: Vec<NotchFilter>,
        slew_rate: Gain,
        curve_points: Vec<CurvePoint>,
    ) -> Result<Self, DomainError> {
        // Validate reconstruction level
        if reconstruction > 8 {
            return Err(DomainError::InvalidCurvePoints(format!(
                "Reconstruction level must be 0-8, got {}",
                reconstruction
            )));
        }

        // Validate curve points are monotonic
        validate_curve_monotonic(&curve_points)?;

        Ok(Self {
            reconstruction,
            friction,
            damper,
            inertia,
            notch_filters,
            slew_rate,
            curve_points,
            torque_cap: match Gain::new(1.0) {
                Ok(v) => v,
                Err(e) => panic!("1.0 is a valid gain: {:?}", e),
            }, // No cap by default
            bumpstop: BumpstopConfig::default(),
            hands_off: HandsOffConfig::default(),
        })
    }

    /// Create a new filter configuration with all parameters
    #[allow(clippy::too_many_arguments)]
    pub fn new_complete(
        reconstruction: u8,
        friction: Gain,
        damper: Gain,
        inertia: Gain,
        notch_filters: Vec<NotchFilter>,
        slew_rate: Gain,
        curve_points: Vec<CurvePoint>,
        torque_cap: Gain,
        bumpstop: BumpstopConfig,
        hands_off: HandsOffConfig,
    ) -> Result<Self, DomainError> {
        // Validate reconstruction level
        if reconstruction > 8 {
            return Err(DomainError::InvalidCurvePoints(format!(
                "Reconstruction level must be 0-8, got {}",
                reconstruction
            )));
        }

        // Validate curve points are monotonic
        validate_curve_monotonic(&curve_points)?;

        Ok(Self {
            reconstruction,
            friction,
            damper,
            inertia,
            notch_filters,
            slew_rate,
            curve_points,
            torque_cap,
            bumpstop,
            hands_off,
        })
    }

    /// Check if this is a linear configuration (no curve modification)
    pub fn is_linear(&self) -> bool {
        self.curve_points.len() == 2
            && self.curve_points[0].input == 0.0
            && self.curve_points[0].output == 0.0
            && self.curve_points[1].input == 1.0
            && self.curve_points[1].output == 1.0
    }
}

/// Base settings for wheel configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BaseSettings {
    /// Overall force feedback gain
    pub ffb_gain: Gain,

    /// Degrees of rotation (steering lock)
    pub degrees_of_rotation: Degrees,

    /// Maximum torque limit
    pub torque_cap: TorqueNm,

    /// Filter configuration
    pub filters: FilterConfig,
}

impl BaseSettings {
    /// Create new base settings with validation
    pub fn new(
        ffb_gain: Gain,
        degrees_of_rotation: Degrees,
        torque_cap: TorqueNm,
        filters: FilterConfig,
    ) -> Self {
        Self {
            ffb_gain,
            degrees_of_rotation,
            torque_cap,
            filters,
        }
    }

    /// Validate settings against device capabilities
    pub fn validate_for_device(
        &self,
        capabilities: &DeviceCapabilities,
    ) -> Result<(), DomainError> {
        // Check torque cap doesn't exceed device maximum
        if self.torque_cap.value() > capabilities.max_torque.value() {
            return Err(DomainError::InvalidTorque(
                self.torque_cap.value(),
                capabilities.max_torque.value(),
            ));
        }

        Ok(())
    }
}

impl Default for BaseSettings {
    fn default() -> Self {
        Self {
            ffb_gain: match Gain::new(0.7) {
                Ok(v) => v,
                Err(e) => panic!("0.7 is a valid gain: {:?}", e),
            },
            degrees_of_rotation: match Degrees::new_dor(900.0) {
                Ok(v) => v,
                Err(e) => panic!("900.0 is a valid DOR: {:?}", e),
            },
            torque_cap: match TorqueNm::new(15.0) {
                Ok(v) => v,
                Err(e) => panic!("15.0 is a valid torque: {:?}", e),
            },
            filters: FilterConfig::default(),
        }
    }
}

/// LED configuration for wheel lighting
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LedConfig {
    /// RPM threshold bands for shift lights (0.0-1.0 as fraction of redline)
    pub rpm_bands: Vec<f32>,

    /// LED pattern name
    pub pattern: String,

    /// Overall brightness (0.0-1.0)
    pub brightness: Gain,

    /// Color mapping for different states
    pub colors: HashMap<String, [u8; 3]>, // RGB values
}

impl LedConfig {
    /// Create new LED configuration
    pub fn new(
        rpm_bands: Vec<f32>,
        pattern: String,
        brightness: Gain,
        colors: HashMap<String, [u8; 3]>,
    ) -> Result<Self, DomainError> {
        // Validate RPM bands are in valid range and sorted
        for &band in &rpm_bands {
            if !(0.0..=1.0).contains(&band) {
                return Err(DomainError::InvalidCurvePoints(format!(
                    "RPM band must be 0.0-1.0, got {}",
                    band
                )));
            }
        }

        // Check if bands are sorted
        for window in rpm_bands.windows(2) {
            if window[1] <= window[0] {
                return Err(DomainError::InvalidCurvePoints(
                    "RPM bands must be in ascending order".to_string(),
                ));
            }
        }

        Ok(Self {
            rpm_bands,
            pattern,
            brightness,
            colors,
        })
    }
}

impl Default for LedConfig {
    fn default() -> Self {
        let mut colors = HashMap::new();
        colors.insert("green".to_string(), [0, 255, 0]);
        colors.insert("yellow".to_string(), [255, 255, 0]);
        colors.insert("red".to_string(), [255, 0, 0]);
        colors.insert("blue".to_string(), [0, 0, 255]);

        Self {
            rpm_bands: vec![0.75, 0.82, 0.88, 0.92, 0.96],
            pattern: "progressive".to_string(),
            brightness: match Gain::new(0.8) {
                Ok(v) => v,
                Err(e) => panic!("0.8 is a valid gain: {:?}", e),
            },
            colors,
        }
    }
}

/// Haptics configuration for tactile feedback
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HapticsConfig {
    /// Enable haptic feedback
    pub enabled: bool,

    /// Overall haptic intensity (0.0-1.0)
    pub intensity: Gain,

    /// Base frequency for haptic effects
    pub frequency: FrequencyHz,

    /// Enable specific haptic effects
    pub effects: HashMap<String, bool>,
}

impl HapticsConfig {
    /// Create new haptics configuration
    pub fn new(
        enabled: bool,
        intensity: Gain,
        frequency: FrequencyHz,
        effects: HashMap<String, bool>,
    ) -> Self {
        Self {
            enabled,
            intensity,
            frequency,
            effects,
        }
    }
}

impl Default for HapticsConfig {
    fn default() -> Self {
        let mut effects = HashMap::new();
        effects.insert("kerb".to_string(), true);
        effects.insert("slip".to_string(), true);
        effects.insert("gear_shift".to_string(), false);
        effects.insert("collision".to_string(), true);

        Self {
            enabled: true,
            intensity: match Gain::new(0.6) {
                Ok(v) => v,
                Err(e) => panic!("0.6 is a valid gain: {:?}", e),
            },
            frequency: match FrequencyHz::new(80.0) {
                Ok(v) => v,
                Err(e) => panic!("80.0 is a valid frequency: {:?}", e),
            },
            effects,
        }
    }
}

/// Profile scope defines when a profile applies
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ProfileScope {
    /// Game/simulator name (None = applies to all games)
    pub game: Option<String>,

    /// Car identifier (None = applies to all cars in game)
    pub car: Option<String>,

    /// Track identifier (None = applies to all tracks)
    pub track: Option<String>,
}

impl ProfileScope {
    /// Create a global profile scope (applies everywhere)
    pub fn global() -> Self {
        Self {
            game: None,
            car: None,
            track: None,
        }
    }

    /// Create a game-specific profile scope
    pub fn for_game(game: String) -> Self {
        Self {
            game: Some(game),
            car: None,
            track: None,
        }
    }

    /// Create a car-specific profile scope
    pub fn for_car(game: String, car: String) -> Self {
        Self {
            game: Some(game),
            car: Some(car),
            track: None,
        }
    }

    /// Create a track-specific profile scope
    pub fn for_track(game: String, car: String, track: String) -> Self {
        Self {
            game: Some(game),
            car: Some(car),
            track: Some(track),
        }
    }

    /// Check if this scope is more specific than another
    pub fn is_more_specific_than(&self, other: &ProfileScope) -> bool {
        let self_specificity = self.specificity_level();
        let other_specificity = other.specificity_level();
        self_specificity > other_specificity
    }

    /// Get the specificity level (0 = global, 3 = most specific)
    pub fn specificity_level(&self) -> u8 {
        let mut level = 0;
        if self.game.is_some() {
            level += 1;
        }
        if self.car.is_some() {
            level += 1;
        }
        if self.track.is_some() {
            level += 1;
        }
        level
    }

    /// Check if this scope matches a given context
    pub fn matches(&self, game: Option<&str>, car: Option<&str>, track: Option<&str>) -> bool {
        // Check game match
        if let Some(ref scope_game) = self.game
            && game != Some(scope_game.as_str())
        {
            return false;
        }

        // Check car match
        if let Some(ref scope_car) = self.car
            && car != Some(scope_car.as_str())
        {
            return false;
        }

        // Check track match
        if let Some(ref scope_track) = self.track
            && track != Some(scope_track.as_str())
        {
            return false;
        }

        true
    }
}

/// Maximum depth for profile inheritance chains
pub const MAX_INHERITANCE_DEPTH: usize = 5;

/// Fully resolved profile after inheritance chain resolution
///
/// This struct contains the effective settings after walking up the
/// inheritance tree and merging all parent profiles.
#[derive(Debug, Clone)]
pub struct ResolvedProfile {
    /// The effective settings after merging all profiles in the inheritance chain
    pub effective_settings: BaseSettings,

    /// The inheritance chain from child to root (child first, root last)
    /// This is useful for debugging and understanding how settings were derived
    pub inheritance_chain: Vec<ProfileId>,

    /// The effective LED configuration after inheritance
    pub led_config: Option<LedConfig>,

    /// The effective haptics configuration after inheritance
    pub haptics_config: Option<HapticsConfig>,
}

/// Trait for profile storage that supports inheritance resolution
///
/// This trait abstracts the storage mechanism for profiles, allowing
/// the inheritance resolution logic to work with any storage backend.
pub trait ProfileStore {
    /// Get a profile by its ID
    fn get(&self, id: &ProfileId) -> Option<&Profile>;
}

/// Event type for profile change notifications
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProfileChangeEvent {
    /// A profile was modified
    Modified {
        /// The ID of the profile that was modified
        profile_id: ProfileId,
    },
    /// A profile was removed
    Removed {
        /// The ID of the profile that was removed
        profile_id: ProfileId,
    },
}

/// Trait for observing profile changes
///
/// Implement this trait to receive notifications when profiles change.
/// This is particularly useful for child profiles that need to know
/// when their parent profile has been modified.
pub trait ProfileChangeObserver: Send + Sync {
    /// Called when a profile change event occurs
    ///
    /// # Arguments
    /// * `event` - The change event that occurred
    /// * `affected_children` - List of child profile IDs that inherit from the changed profile
    fn on_profile_change(&self, event: &ProfileChangeEvent, affected_children: &[ProfileId]);
}

/// Simple in-memory profile store implementation
///
/// This is useful for testing and simple use cases where profiles
/// are loaded into memory.
///
/// The store supports an observer pattern for profile changes. When a profile
/// is modified or removed, registered observers are notified along with a list
/// of child profiles that may be affected by the change.
#[derive(Default)]
pub struct InMemoryProfileStore {
    profiles: std::collections::HashMap<ProfileId, Profile>,
    observers: Vec<std::sync::Arc<dyn ProfileChangeObserver>>,
}

impl InMemoryProfileStore {
    /// Create a new empty profile store
    pub fn new() -> Self {
        Self {
            profiles: std::collections::HashMap::new(),
            observers: Vec::new(),
        }
    }

    /// Add a profile to the store
    pub fn add(&mut self, profile: Profile) {
        self.profiles.insert(profile.id.clone(), profile);
    }

    /// Remove a profile from the store
    ///
    /// If the profile exists and has child profiles, observers will be notified
    /// with the list of affected children.
    pub fn remove(&mut self, id: &ProfileId) -> Option<Profile> {
        let removed = self.profiles.remove(id);
        if removed.is_some() {
            let affected_children = self.find_children(id);
            self.notify_observers(
                &ProfileChangeEvent::Removed {
                    profile_id: id.clone(),
                },
                &affected_children,
            );
        }
        removed
    }

    /// Get a mutable reference to a profile
    pub fn get_mut(&mut self, id: &ProfileId) -> Option<&mut Profile> {
        self.profiles.get_mut(id)
    }

    /// Get the number of profiles in the store
    pub fn len(&self) -> usize {
        self.profiles.len()
    }

    /// Check if the store is empty
    pub fn is_empty(&self) -> bool {
        self.profiles.is_empty()
    }

    /// Iterate over all profiles
    pub fn iter(&self) -> impl Iterator<Item = (&ProfileId, &Profile)> {
        self.profiles.iter()
    }

    /// Register an observer to receive profile change notifications
    ///
    /// Observers are notified when:
    /// - A profile is modified via `update()`
    /// - A profile is removed via `remove()`
    ///
    /// The notification includes a list of child profiles that inherit from
    /// the changed profile, allowing them to re-resolve their inheritance.
    ///
    /// # Arguments
    /// * `observer` - The observer to register
    pub fn register_observer(&mut self, observer: std::sync::Arc<dyn ProfileChangeObserver>) {
        self.observers.push(observer);
    }

    /// Unregister all observers
    ///
    /// This is useful for cleanup or when you want to temporarily disable
    /// notifications.
    pub fn clear_observers(&mut self) {
        self.observers.clear();
    }

    /// Get the number of registered observers
    pub fn observer_count(&self) -> usize {
        self.observers.len()
    }

    /// Update a profile in the store and notify observers
    ///
    /// This method should be used when modifying a profile's settings to ensure
    /// that child profiles are notified of the change.
    ///
    /// # Arguments
    /// * `profile` - The updated profile
    ///
    /// # Returns
    /// * `Some(Profile)` - The previous version of the profile if it existed
    /// * `None` - If this is a new profile
    pub fn update(&mut self, profile: Profile) -> Option<Profile> {
        let profile_id = profile.id.clone();
        let previous = self.profiles.insert(profile_id.clone(), profile);

        // Notify observers of the change
        let affected_children = self.find_children(&profile_id);
        self.notify_observers(
            &ProfileChangeEvent::Modified { profile_id },
            &affected_children,
        );

        previous
    }

    /// Find all profiles that directly inherit from the given profile
    ///
    /// This returns only direct children (one level deep). For a full
    /// inheritance tree, you would need to recursively call this method.
    ///
    /// # Arguments
    /// * `parent_id` - The ID of the parent profile
    ///
    /// # Returns
    /// A vector of profile IDs that have the given profile as their parent
    pub fn find_children(&self, parent_id: &ProfileId) -> Vec<ProfileId> {
        self.profiles
            .values()
            .filter(|p| p.parent.as_ref() == Some(parent_id))
            .map(|p| p.id.clone())
            .collect()
    }

    /// Find all profiles in the inheritance tree below the given profile
    ///
    /// This returns all descendants (children, grandchildren, etc.) of the
    /// given profile.
    ///
    /// # Arguments
    /// * `parent_id` - The ID of the root profile
    ///
    /// # Returns
    /// A vector of all descendant profile IDs
    pub fn find_all_descendants(&self, parent_id: &ProfileId) -> Vec<ProfileId> {
        let mut descendants = Vec::new();
        let mut to_visit = vec![parent_id.clone()];

        while let Some(current_id) = to_visit.pop() {
            let children = self.find_children(&current_id);
            for child_id in children {
                descendants.push(child_id.clone());
                to_visit.push(child_id);
            }
        }

        descendants
    }

    /// Notify all registered observers of a profile change
    fn notify_observers(&self, event: &ProfileChangeEvent, affected_children: &[ProfileId]) {
        for observer in &self.observers {
            observer.on_profile_change(event, affected_children);
        }
    }
}

impl ProfileStore for InMemoryProfileStore {
    fn get(&self, id: &ProfileId) -> Option<&Profile> {
        self.profiles.get(id)
    }
}

/// Complete profile configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Unique profile identifier
    pub id: ProfileId,

    /// Parent profile for inheritance (optional)
    /// When set, this profile inherits settings from the parent profile.
    /// Child values override parent values, unspecified values inherit from parent.
    /// Inheritance chains are limited to 5 levels deep.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent: Option<ProfileId>,

    /// Profile scope (when it applies)
    pub scope: ProfileScope,

    /// Base wheel settings
    pub base_settings: BaseSettings,

    /// LED configuration (optional)
    pub led_config: Option<LedConfig>,

    /// Haptics configuration (optional)
    pub haptics_config: Option<HapticsConfig>,

    /// Profile metadata
    pub metadata: ProfileMetadata,
}

/// Profile metadata
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileMetadata {
    /// Human-readable profile name
    pub name: String,

    /// Profile description
    pub description: Option<String>,

    /// Profile author
    pub author: Option<String>,

    /// Profile version
    pub version: String,

    /// Creation timestamp (ISO 8601)
    pub created_at: String,

    /// Last modified timestamp (ISO 8601)
    pub modified_at: String,

    /// Profile tags for organization
    pub tags: Vec<String>,
}

impl Profile {
    /// Create a new profile
    pub fn new(
        id: ProfileId,
        scope: ProfileScope,
        base_settings: BaseSettings,
        name: String,
    ) -> Self {
        let now = chrono::Utc::now().to_rfc3339();

        Self {
            id,
            parent: None,
            scope,
            base_settings,
            led_config: Some(LedConfig::default()),
            haptics_config: Some(HapticsConfig::default()),
            metadata: ProfileMetadata {
                name,
                description: None,
                author: None,
                version: "1.0.0".to_string(),
                created_at: now.clone(),
                modified_at: now,
                tags: Vec::new(),
            },
        }
    }

    /// Create a new profile with a parent for inheritance
    pub fn new_with_parent(
        id: ProfileId,
        parent: ProfileId,
        scope: ProfileScope,
        base_settings: BaseSettings,
        name: String,
    ) -> Self {
        let mut profile = Self::new(id, scope, base_settings, name);
        profile.parent = Some(parent);
        profile
    }

    /// Set the parent profile for inheritance
    ///
    /// When a parent is set, this profile will inherit settings from the parent.
    /// Child values override parent values, unspecified values inherit from parent.
    pub fn set_parent(&mut self, parent: Option<ProfileId>) {
        self.parent = parent;
        self.metadata.modified_at = chrono::Utc::now().to_rfc3339();
    }

    /// Get the parent profile ID if set
    pub fn parent(&self) -> Option<&ProfileId> {
        self.parent.as_ref()
    }

    /// Check if this profile has a parent (is a child profile)
    pub fn has_parent(&self) -> bool {
        self.parent.is_some()
    }

    /// Create a global default profile
    pub fn default_global() -> Result<Self, DomainError> {
        let id: ProfileId = "global".parse()?;
        Ok(Self::new(
            id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Global Default".to_string(),
        ))
    }

    /// Merge this profile with another, with the other taking precedence
    pub fn merge_with(&self, other: &Profile) -> Self {
        let mut merged = self.clone();

        // Base settings merge (other takes precedence for non-default values)
        merged.base_settings = other.base_settings.clone();

        // LED config merge
        if other.led_config.is_some() {
            merged.led_config = other.led_config.clone();
        }

        // Haptics config merge
        if other.haptics_config.is_some() {
            merged.haptics_config = other.haptics_config.clone();
        }

        // Update metadata
        merged.metadata.modified_at = chrono::Utc::now().to_rfc3339();

        merged
    }

    /// Merge this child profile with a parent profile for inheritance
    ///
    /// This implements profile inheritance where:
    /// - Child values override parent values
    /// - Unspecified/default values in child inherit from parent
    /// - The merge is deterministic (same inputs → same output)
    ///
    /// # Arguments
    /// * `parent` - The parent profile to inherit from
    ///
    /// # Returns
    /// A new profile with merged settings where child values take precedence
    ///
    /// # Example
    /// ```ignore
    /// let parent = Profile::default_global()?;
    /// let child = Profile::new_with_parent(child_id, parent.id.clone(), scope, settings, name);
    /// let merged = child.merge_with_parent(&parent);
    /// // merged has child's explicit values, parent's values for unspecified fields
    /// ```
    pub fn merge_with_parent(&self, parent: &Profile) -> Self {
        let mut merged = parent.clone();

        // Keep child's identity
        merged.id = self.id.clone();
        merged.parent = self.parent.clone();
        merged.scope = self.scope.clone();
        merged.metadata = self.metadata.clone();

        // Merge base settings - child values override parent values
        // Only override if child has non-default values
        Self::merge_base_settings_with_parent(&mut merged.base_settings, &self.base_settings);

        // LED config: child overrides if present
        if self.led_config.is_some() {
            merged.led_config = self.led_config.clone();
        }

        // Haptics config: child overrides if present
        if self.haptics_config.is_some() {
            merged.haptics_config = self.haptics_config.clone();
        }

        // Update modified timestamp
        merged.metadata.modified_at = chrono::Utc::now().to_rfc3339();

        merged
    }

    /// Merge base settings from child into parent, with child values overriding
    ///
    /// This method compares child values against defaults to determine which
    /// values were explicitly set by the child vs inherited from defaults.
    fn merge_base_settings_with_parent(target: &mut BaseSettings, child: &BaseSettings) {
        let defaults = BaseSettings::default();

        // FFB gain: child overrides if different from default
        if !Self::is_approximately_equal(child.ffb_gain.value(), defaults.ffb_gain.value()) {
            target.ffb_gain = child.ffb_gain;
        }

        // Degrees of rotation: child overrides if different from default
        if !Self::is_approximately_equal(
            child.degrees_of_rotation.value(),
            defaults.degrees_of_rotation.value(),
        ) {
            target.degrees_of_rotation = child.degrees_of_rotation;
        }

        // Torque cap: child overrides if different from default
        if !Self::is_approximately_equal(child.torque_cap.value(), defaults.torque_cap.value()) {
            target.torque_cap = child.torque_cap;
        }

        // Merge filter configuration
        Self::merge_filter_config_with_parent(&mut target.filters, &child.filters);
    }

    /// Merge filter configuration from child into parent
    fn merge_filter_config_with_parent(target: &mut FilterConfig, child: &FilterConfig) {
        let defaults = FilterConfig::default();

        // Reconstruction: child overrides if different from default
        if child.reconstruction != defaults.reconstruction {
            target.reconstruction = child.reconstruction;
        }

        // Friction: child overrides if different from default
        if !Self::is_approximately_equal(child.friction.value(), defaults.friction.value()) {
            target.friction = child.friction;
        }

        // Damper: child overrides if different from default
        if !Self::is_approximately_equal(child.damper.value(), defaults.damper.value()) {
            target.damper = child.damper;
        }

        // Inertia: child overrides if different from default
        if !Self::is_approximately_equal(child.inertia.value(), defaults.inertia.value()) {
            target.inertia = child.inertia;
        }

        // Slew rate: child overrides if different from default
        if !Self::is_approximately_equal(child.slew_rate.value(), defaults.slew_rate.value()) {
            target.slew_rate = child.slew_rate;
        }

        // Torque cap (filter level): child overrides if different from default
        if !Self::is_approximately_equal(child.torque_cap.value(), defaults.torque_cap.value()) {
            target.torque_cap = child.torque_cap;
        }

        // Notch filters: child overrides if non-empty
        if !child.notch_filters.is_empty() {
            target.notch_filters = child.notch_filters.clone();
        }

        // Curve points: child overrides if not the default linear curve
        if !Self::is_default_linear_curve(&child.curve_points) {
            target.curve_points = child.curve_points.clone();
        }

        // Bumpstop: child overrides if different from default
        if !Self::is_default_bumpstop(&child.bumpstop) {
            target.bumpstop = child.bumpstop.clone();
        }

        // Hands-off: child overrides if different from default
        if !Self::is_default_hands_off(&child.hands_off) {
            target.hands_off = child.hands_off.clone();
        }
    }

    /// Check if two f32 values are approximately equal (within epsilon)
    #[inline]
    fn is_approximately_equal(a: f32, b: f32) -> bool {
        (a - b).abs() < f32::EPSILON
    }

    /// Check if curve points represent the default linear curve
    fn is_default_linear_curve(curve_points: &[CurvePoint]) -> bool {
        curve_points.len() == 2
            && Self::is_approximately_equal(curve_points[0].input, 0.0)
            && Self::is_approximately_equal(curve_points[0].output, 0.0)
            && Self::is_approximately_equal(curve_points[1].input, 1.0)
            && Self::is_approximately_equal(curve_points[1].output, 1.0)
    }

    /// Check if bumpstop config is the default
    fn is_default_bumpstop(bumpstop: &BumpstopConfig) -> bool {
        let defaults = BumpstopConfig::default();
        bumpstop.enabled == defaults.enabled
            && Self::is_approximately_equal(bumpstop.start_angle, defaults.start_angle)
            && Self::is_approximately_equal(bumpstop.max_angle, defaults.max_angle)
            && Self::is_approximately_equal(bumpstop.stiffness, defaults.stiffness)
            && Self::is_approximately_equal(bumpstop.damping, defaults.damping)
    }

    /// Check if hands-off config is the default
    fn is_default_hands_off(hands_off: &HandsOffConfig) -> bool {
        let defaults = HandsOffConfig::default();
        hands_off.enabled == defaults.enabled
            && Self::is_approximately_equal(hands_off.threshold, defaults.threshold)
            && Self::is_approximately_equal(hands_off.timeout_seconds, defaults.timeout_seconds)
    }

    /// Validate profile against device capabilities
    pub fn validate_for_device(
        &self,
        capabilities: &DeviceCapabilities,
    ) -> Result<(), DomainError> {
        self.base_settings.validate_for_device(capabilities)
    }

    /// Calculate a hash for deterministic profile comparison
    pub fn calculate_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();

        // Hash the settings that affect behavior (not metadata)
        self.base_settings
            .ffb_gain
            .value()
            .to_bits()
            .hash(&mut hasher);
        self.base_settings
            .degrees_of_rotation
            .value()
            .to_bits()
            .hash(&mut hasher);
        self.base_settings
            .torque_cap
            .value()
            .to_bits()
            .hash(&mut hasher);

        // Hash filter config
        self.base_settings.filters.reconstruction.hash(&mut hasher);
        self.base_settings
            .filters
            .friction
            .value()
            .to_bits()
            .hash(&mut hasher);
        self.base_settings
            .filters
            .damper
            .value()
            .to_bits()
            .hash(&mut hasher);
        self.base_settings
            .filters
            .inertia
            .value()
            .to_bits()
            .hash(&mut hasher);
        self.base_settings
            .filters
            .slew_rate
            .value()
            .to_bits()
            .hash(&mut hasher);

        // Hash curve points
        for point in &self.base_settings.filters.curve_points {
            point.input.to_bits().hash(&mut hasher);
            point.output.to_bits().hash(&mut hasher);
        }

        // Hash notch filters
        for filter in &self.base_settings.filters.notch_filters {
            filter.frequency.value().to_bits().hash(&mut hasher);
            filter.q_factor.to_bits().hash(&mut hasher);
            filter.gain_db.to_bits().hash(&mut hasher);
        }

        hasher.finish()
    }

    /// Resolve the inheritance chain for this profile
    ///
    /// This method walks up the inheritance tree, merging settings from each
    /// parent profile until it reaches a profile with no parent or the maximum
    /// depth is exceeded.
    ///
    /// # Arguments
    /// * `store` - A profile store that can look up profiles by ID
    ///
    /// # Returns
    /// * `Ok(ResolvedProfile)` - The fully resolved profile with effective settings
    /// * `Err(DomainError::InheritanceDepthExceeded)` - If the chain exceeds 5 levels
    /// * `Err(DomainError::CircularInheritance)` - If a circular reference is detected
    /// * `Err(DomainError::ParentProfileNotFound)` - If a parent profile is not found
    ///
    /// # Example
    /// ```ignore
    /// let store = InMemoryProfileStore::new();
    /// // ... add profiles to store ...
    /// let resolved = child_profile.resolve(&store)?;
    /// // resolved.effective_settings contains the merged settings
    /// // resolved.inheritance_chain contains [child_id, parent_id, grandparent_id, ...]
    /// ```
    pub fn resolve<S: ProfileStore>(&self, store: &S) -> Result<ResolvedProfile, DomainError> {
        let mut inheritance_chain = Vec::with_capacity(MAX_INHERITANCE_DEPTH + 1);
        let mut visited = std::collections::HashSet::new();

        // Start with this profile
        inheritance_chain.push(self.id.clone());
        visited.insert(self.id.clone());

        // Collect all profiles in the inheritance chain
        let mut profiles_to_merge: Vec<&Profile> = vec![self];
        let mut current_profile = self;

        while let Some(parent_id) = &current_profile.parent {
            // Check for circular inheritance
            if visited.contains(parent_id) {
                return Err(DomainError::CircularInheritance {
                    profile_id: parent_id.to_string(),
                });
            }

            // Check depth limit (we already have profiles_to_merge.len() profiles)
            if profiles_to_merge.len() >= MAX_INHERITANCE_DEPTH {
                return Err(DomainError::InheritanceDepthExceeded {
                    depth: profiles_to_merge.len() + 1,
                    max_depth: MAX_INHERITANCE_DEPTH,
                });
            }

            // Look up the parent profile
            let parent =
                store
                    .get(parent_id)
                    .ok_or_else(|| DomainError::ParentProfileNotFound {
                        profile_id: parent_id.to_string(),
                    })?;

            inheritance_chain.push(parent_id.clone());
            visited.insert(parent_id.clone());
            profiles_to_merge.push(parent);
            current_profile = parent;
        }

        // Now merge from root to child (reverse order)
        // Start with the root profile's settings
        let root_profile = profiles_to_merge
            .last()
            .ok_or_else(|| DomainError::InvalidProfileId("Empty inheritance chain".to_string()))?;

        let mut effective_settings = root_profile.base_settings.clone();
        let mut effective_led_config = root_profile.led_config.clone();
        let mut effective_haptics_config = root_profile.haptics_config.clone();

        // Merge from second-to-last to first (child)
        // Skip the root (last) since we already have its settings
        for profile in profiles_to_merge.iter().rev().skip(1) {
            // Merge base settings - child values override parent values
            Self::merge_base_settings_with_parent(&mut effective_settings, &profile.base_settings);

            // LED config: child overrides if present
            if profile.led_config.is_some() {
                effective_led_config = profile.led_config.clone();
            }

            // Haptics config: child overrides if present
            if profile.haptics_config.is_some() {
                effective_haptics_config = profile.haptics_config.clone();
            }
        }

        Ok(ResolvedProfile {
            effective_settings,
            inheritance_chain,
            led_config: effective_led_config,
            haptics_config: effective_haptics_config,
        })
    }

    /// Validate that the inheritance chain is valid without fully resolving it
    ///
    /// This is a lighter-weight check that verifies:
    /// - No circular inheritance
    /// - Depth does not exceed the maximum
    /// - All parent profiles exist
    ///
    /// # Arguments
    /// * `store` - A profile store that can look up profiles by ID
    ///
    /// # Returns
    /// * `Ok(())` - The inheritance chain is valid
    /// * `Err(DomainError)` - The chain is invalid (same errors as resolve())
    pub fn validate_inheritance<S: ProfileStore>(&self, store: &S) -> Result<(), DomainError> {
        // Simply call resolve and discard the result
        // This ensures validation logic is consistent with resolution
        self.resolve(store).map(|_| ())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper function to unwrap Result values in tests
    /// Panics with a descriptive message if the Result is Err
    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("must() failed: {:?}", e),
        }
    }

    #[test]
    fn test_device_capabilities() {
        let caps = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            must(TorqueNm::new(25.0)),
            10000,
            1000,
        );

        assert!(caps.supports_ffb());
        assert_eq!(caps.max_update_rate_hz(), 1000.0);
    }

    #[test]
    fn test_device_creation() {
        let id = must("test-device".parse::<DeviceId>());
        let caps = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            must(TorqueNm::new(25.0)),
            10000,
            1000,
        );

        let device = Device::new(
            id.clone(),
            "Test Wheel".to_string(),
            DeviceType::WheelBase,
            caps,
        );

        assert_eq!(device.id, id);
        assert_eq!(device.name, "Test Wheel");
        assert_eq!(device.device_type, DeviceType::WheelBase);
        assert_eq!(device.state, DeviceState::Connected);
        assert!(!device.has_faults());
    }

    #[test]
    fn test_device_fault_handling() {
        let id = must("test-device".parse::<DeviceId>());
        let caps = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            must(TorqueNm::new(25.0)),
            10000,
            1000,
        );

        let mut device = Device::new(id, "Test Wheel".to_string(), DeviceType::WheelBase, caps);

        // Set fault
        device.set_fault_flags(0x04); // Thermal fault
        assert!(device.has_faults());
        assert_eq!(device.state, DeviceState::Faulted);
        assert!(!device.is_operational());

        // Clear fault
        device.clear_faults();
        assert!(!device.has_faults());
        assert_eq!(device.state, DeviceState::Active);
    }

    #[test]
    fn test_filter_config_validation() {
        // Valid filter config
        let config = FilterConfig::new(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
        );
        assert!(config.is_ok());

        // Invalid reconstruction level
        let bad_config = FilterConfig::new(
            10, // Too high
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
        );
        assert!(bad_config.is_err());
    }

    #[test]
    fn test_profile_scope_matching() {
        let global_scope = ProfileScope::global();
        let game_scope = ProfileScope::for_game("iracing".to_string());
        let car_scope = ProfileScope::for_car("iracing".to_string(), "gt3".to_string());

        // Global scope matches everything
        assert!(global_scope.matches(None, None, None));
        assert!(global_scope.matches(Some("iracing"), None, None));
        assert!(global_scope.matches(Some("iracing"), Some("gt3"), None));

        // Game scope matches only that game
        assert!(!game_scope.matches(None, None, None));
        assert!(game_scope.matches(Some("iracing"), None, None));
        assert!(game_scope.matches(Some("iracing"), Some("gt3"), None));
        assert!(!game_scope.matches(Some("acc"), None, None));

        // Car scope matches only that game+car combination
        assert!(!car_scope.matches(Some("iracing"), None, None));
        assert!(car_scope.matches(Some("iracing"), Some("gt3"), None));
        assert!(!car_scope.matches(Some("iracing"), Some("f1"), None));
    }

    #[test]
    fn test_profile_scope_specificity() {
        let global_scope = ProfileScope::global();
        let game_scope = ProfileScope::for_game("iracing".to_string());
        let car_scope = ProfileScope::for_car("iracing".to_string(), "gt3".to_string());
        let track_scope =
            ProfileScope::for_track("iracing".to_string(), "gt3".to_string(), "spa".to_string());

        assert_eq!(global_scope.specificity_level(), 0);
        assert_eq!(game_scope.specificity_level(), 1);
        assert_eq!(car_scope.specificity_level(), 2);
        assert_eq!(track_scope.specificity_level(), 3);

        assert!(game_scope.is_more_specific_than(&global_scope));
        assert!(car_scope.is_more_specific_than(&game_scope));
        assert!(track_scope.is_more_specific_than(&car_scope));
    }

    #[test]
    fn test_profile_creation() {
        let id = must("test-profile".parse::<ProfileId>());
        let scope = ProfileScope::for_game("iracing".to_string());
        let base_settings = BaseSettings::default();

        let profile = Profile::new(id.clone(), scope, base_settings, "Test Profile".to_string());

        assert_eq!(profile.id, id);
        assert_eq!(profile.metadata.name, "Test Profile");
        assert!(profile.led_config.is_some());
        assert!(profile.haptics_config.is_some());
    }

    #[test]
    fn test_profile_hash_deterministic() {
        let id = must("test-profile".parse::<ProfileId>());
        let scope = ProfileScope::for_game("iracing".to_string());
        let base_settings = BaseSettings::default();

        let profile1 = Profile::new(
            id.clone(),
            scope.clone(),
            base_settings.clone(),
            "Test Profile".to_string(),
        );

        let profile2 = Profile::new(
            id,
            scope,
            base_settings,
            "Different Name".to_string(), // Different metadata
        );

        // Hash should be the same despite different metadata
        assert_eq!(profile1.calculate_hash(), profile2.calculate_hash());
    }

    #[test]
    fn test_led_config_validation() {
        // Valid LED config
        let config = LedConfig::new(
            vec![0.75, 0.82, 0.88, 0.92, 0.96],
            "progressive".to_string(),
            must(Gain::new(0.8)),
            HashMap::new(),
        );
        assert!(config.is_ok());

        // Invalid RPM bands (not sorted)
        let bad_config = LedConfig::new(
            vec![0.75, 0.92, 0.82], // Not sorted
            "progressive".to_string(),
            must(Gain::new(0.8)),
            HashMap::new(),
        );
        assert!(bad_config.is_err());

        // Invalid RPM bands (out of range)
        let bad_config2 = LedConfig::new(
            vec![0.75, 1.2], // > 1.0
            "progressive".to_string(),
            must(Gain::new(0.8)),
            HashMap::new(),
        );
        assert!(bad_config2.is_err());
    }

    #[test]
    fn test_notch_filter_validation() {
        let freq = must(FrequencyHz::new(60.0));

        // Valid notch filter
        let filter = NotchFilter::new(freq, 2.0, -12.0);
        assert!(filter.is_ok());

        // Invalid Q factor
        let bad_filter = NotchFilter::new(freq, 0.0, -12.0);
        assert!(bad_filter.is_err());

        // Invalid gain (NaN)
        let bad_filter2 = NotchFilter::new(freq, 2.0, f32::NAN);
        assert!(bad_filter2.is_err());
    }

    #[test]
    fn test_profile_parent_field_default_none() {
        let id = must("child-profile".parse::<ProfileId>());
        let scope = ProfileScope::for_game("iracing".to_string());
        let base_settings = BaseSettings::default();

        let profile = Profile::new(id, scope, base_settings, "Child Profile".to_string());

        // New profiles should have no parent by default
        assert!(profile.parent.is_none());
        assert!(!profile.has_parent());
        assert!(profile.parent().is_none());
    }

    #[test]
    fn test_profile_new_with_parent() {
        let child_id = must("child-profile".parse::<ProfileId>());
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let scope = ProfileScope::for_game("iracing".to_string());
        let base_settings = BaseSettings::default();

        let profile = Profile::new_with_parent(
            child_id.clone(),
            parent_id.clone(),
            scope,
            base_settings,
            "Child Profile".to_string(),
        );

        assert_eq!(profile.id, child_id);
        assert!(profile.has_parent());
        assert_eq!(profile.parent(), Some(&parent_id));
    }

    #[test]
    fn test_profile_set_parent() {
        let id = must("test-profile".parse::<ProfileId>());
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let scope = ProfileScope::for_game("iracing".to_string());
        let base_settings = BaseSettings::default();

        let mut profile = Profile::new(id, scope, base_settings, "Test Profile".to_string());

        // Initially no parent
        assert!(!profile.has_parent());

        // Set parent
        profile.set_parent(Some(parent_id.clone()));
        assert!(profile.has_parent());
        assert_eq!(profile.parent(), Some(&parent_id));

        // Clear parent
        profile.set_parent(None);
        assert!(!profile.has_parent());
        assert!(profile.parent().is_none());
    }

    #[test]
    fn test_profile_parent_serialization_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let child_id = must("child-profile".parse::<ProfileId>());
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let scope = ProfileScope::for_game("iracing".to_string());
        let base_settings = BaseSettings::default();

        let profile = Profile::new_with_parent(
            child_id,
            parent_id.clone(),
            scope,
            base_settings,
            "Child Profile".to_string(),
        );

        // Serialize to JSON
        let json = serde_json::to_string(&profile)?;

        // Deserialize back
        let deserialized: Profile = serde_json::from_str(&json)?;

        // Verify parent is preserved
        assert!(deserialized.has_parent());
        assert_eq!(deserialized.parent(), Some(&parent_id));

        Ok(())
    }

    #[test]
    fn test_profile_without_parent_backward_compatible() -> Result<(), Box<dyn std::error::Error>> {
        // JSON without parent field (simulating old profile format)
        let json_without_parent = r#"{
            "id": "old-profile",
            "scope": {"game": "iracing", "car": null, "track": null},
            "base_settings": {
                "ffb_gain": 0.7,
                "degrees_of_rotation": 900.0,
                "torque_cap": 15.0,
                "filters": {
                    "reconstruction": 0,
                    "friction": 0.0,
                    "damper": 0.0,
                    "inertia": 0.0,
                    "notch_filters": [],
                    "slew_rate": 1.0,
                    "curve_points": [{"input": 0.0, "output": 0.0}, {"input": 1.0, "output": 1.0}],
                    "torque_cap": 1.0,
                    "bumpstop": {"enabled": true, "start_angle": 450.0, "max_angle": 540.0, "stiffness": 0.8, "damping": 0.3},
                    "hands_off": {"enabled": true, "threshold": 0.05, "timeout_seconds": 5.0}
                }
            },
            "led_config": null,
            "haptics_config": null,
            "metadata": {
                "name": "Old Profile",
                "description": null,
                "author": null,
                "version": "1.0.0",
                "created_at": "2024-01-01T00:00:00Z",
                "modified_at": "2024-01-01T00:00:00Z",
                "tags": []
            }
        }"#;

        // Should deserialize successfully without parent field
        let profile: Profile = serde_json::from_str(json_without_parent)?;

        // Parent should be None (backward compatible)
        assert!(!profile.has_parent());
        assert!(profile.parent().is_none());

        Ok(())
    }

    #[test]
    fn test_profile_parent_not_serialized_when_none() -> Result<(), Box<dyn std::error::Error>> {
        let id = must("test-profile".parse::<ProfileId>());
        let scope = ProfileScope::global();
        let base_settings = BaseSettings::default();

        let profile = Profile::new(id, scope, base_settings, "Test Profile".to_string());

        // Serialize to JSON
        let json = serde_json::to_string(&profile)?;

        // Verify "parent" field is not present in JSON (skip_serializing_if = "Option::is_none")
        assert!(!json.contains("\"parent\""));

        Ok(())
    }

    // ==================== Profile Inheritance Merge Tests ====================

    #[test]
    fn test_merge_with_parent_child_overrides_ffb_gain() {
        // Create parent with specific FFB gain
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.ffb_gain = must(Gain::new(0.8));

        // Create child with different FFB gain
        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id.clone(),
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.ffb_gain = must(Gain::new(0.6));

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Child's FFB gain should override parent's
        assert_eq!(merged.base_settings.ffb_gain.value(), 0.6);
        // Child's identity should be preserved
        assert_eq!(merged.id, child_id);
    }

    #[test]
    fn test_merge_with_parent_inherits_unspecified_values() {
        // Create parent with custom settings
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.ffb_gain = must(Gain::new(0.8));
        parent.base_settings.degrees_of_rotation = must(Degrees::new_dor(540.0));
        parent.base_settings.torque_cap = must(TorqueNm::new(20.0));

        // Create child with only default values (unspecified)
        let child_id = must("child-profile".parse::<ProfileId>());
        let child = Profile::new_with_parent(
            child_id.clone(),
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(), // All defaults
            "Child Profile".to_string(),
        );

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Child should inherit parent's non-default values
        assert_eq!(merged.base_settings.ffb_gain.value(), 0.8);
        assert_eq!(merged.base_settings.degrees_of_rotation.value(), 540.0);
        assert_eq!(merged.base_settings.torque_cap.value(), 20.0);
        // Child's identity should be preserved
        assert_eq!(merged.id, child_id);
    }

    #[test]
    fn test_merge_with_parent_partial_override() {
        // Create parent with custom settings
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.ffb_gain = must(Gain::new(0.8));
        parent.base_settings.degrees_of_rotation = must(Degrees::new_dor(540.0));
        parent.base_settings.torque_cap = must(TorqueNm::new(20.0));

        // Create child that only overrides FFB gain
        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id.clone(),
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.ffb_gain = must(Gain::new(0.5)); // Override this

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Child's FFB gain should override
        assert_eq!(merged.base_settings.ffb_gain.value(), 0.5);
        // Parent's other values should be inherited
        assert_eq!(merged.base_settings.degrees_of_rotation.value(), 540.0);
        assert_eq!(merged.base_settings.torque_cap.value(), 20.0);
    }

    #[test]
    fn test_merge_with_parent_filter_config_override() {
        // Create parent with custom filter settings
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.filters.friction = must(Gain::new(0.2));
        parent.base_settings.filters.damper = must(Gain::new(0.3));
        parent.base_settings.filters.reconstruction = 4;

        // Create child that only overrides friction
        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.filters.friction = must(Gain::new(0.5)); // Override this

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Child's friction should override
        assert_eq!(merged.base_settings.filters.friction.value(), 0.5);
        // Parent's other filter values should be inherited
        assert_eq!(merged.base_settings.filters.damper.value(), 0.3);
        assert_eq!(merged.base_settings.filters.reconstruction, 4);
    }

    #[test]
    fn test_merge_with_parent_led_config_override() {
        // Create parent with LED config
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.led_config = Some(LedConfig::default());

        // Create child with different LED config
        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        let child_led = LedConfig {
            brightness: must(Gain::new(0.5)),
            ..LedConfig::default()
        };
        child.led_config = Some(child_led);

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Child's LED config should override
        assert!(merged.led_config.is_some());
        assert_eq!(
            merged.led_config.as_ref().map(|l| l.brightness.value()),
            Some(0.5)
        );
    }

    #[test]
    fn test_merge_with_parent_inherits_led_config() {
        // Create parent with LED config
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        let parent_led = LedConfig {
            brightness: must(Gain::new(0.9)),
            ..LedConfig::default()
        };
        parent.led_config = Some(parent_led);

        // Create child without LED config
        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.led_config = None;

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Parent's LED config should be inherited
        assert!(merged.led_config.is_some());
        assert_eq!(
            merged.led_config.as_ref().map(|l| l.brightness.value()),
            Some(0.9)
        );
    }

    #[test]
    fn test_merge_with_parent_haptics_config_override() {
        // Create parent with haptics config
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.haptics_config = Some(HapticsConfig::default());

        // Create child with different haptics config
        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        let child_haptics = HapticsConfig {
            intensity: must(Gain::new(0.3)),
            ..HapticsConfig::default()
        };
        child.haptics_config = Some(child_haptics);

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Child's haptics config should override
        assert!(merged.haptics_config.is_some());
        assert_eq!(
            merged.haptics_config.as_ref().map(|h| h.intensity.value()),
            Some(0.3)
        );
    }

    #[test]
    fn test_merge_with_parent_preserves_child_identity() {
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let parent = Profile::new(
            parent_id.clone(),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );

        let child_id = must("child-profile".parse::<ProfileId>());
        let child_scope = ProfileScope::for_game("iracing".to_string());
        let child = Profile::new_with_parent(
            child_id.clone(),
            parent_id.clone(),
            child_scope.clone(),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );

        let merged = child.merge_with_parent(&parent);

        // Child's identity should be preserved
        assert_eq!(merged.id, child_id);
        assert_eq!(merged.parent, Some(parent_id));
        assert_eq!(merged.scope, child_scope);
        assert_eq!(merged.metadata.name, "Child Profile");
    }

    #[test]
    fn test_merge_with_parent_deterministic() {
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.ffb_gain = must(Gain::new(0.8));

        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.ffb_gain = must(Gain::new(0.6));

        // Merge multiple times
        let merged1 = child.merge_with_parent(&parent);
        let merged2 = child.merge_with_parent(&parent);

        // Results should be deterministic (same hash)
        assert_eq!(merged1.calculate_hash(), merged2.calculate_hash());
        assert_eq!(
            merged1.base_settings.ffb_gain.value(),
            merged2.base_settings.ffb_gain.value()
        );
    }

    #[test]
    fn test_merge_with_parent_curve_points_override() {
        // Create parent with custom curve
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.filters.curve_points = vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.5, 0.7)),
            must(CurvePoint::new(1.0, 1.0)),
        ];

        // Create child with different curve
        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.filters.curve_points = vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.3, 0.5)),
            must(CurvePoint::new(1.0, 1.0)),
        ];

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Child's curve should override
        assert_eq!(merged.base_settings.filters.curve_points.len(), 3);
        assert_eq!(merged.base_settings.filters.curve_points[1].input, 0.3);
        assert_eq!(merged.base_settings.filters.curve_points[1].output, 0.5);
    }

    #[test]
    fn test_merge_with_parent_inherits_curve_points() {
        // Create parent with custom curve
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.filters.curve_points = vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.5, 0.7)),
            must(CurvePoint::new(1.0, 1.0)),
        ];

        // Create child with default (linear) curve
        let child_id = must("child-profile".parse::<ProfileId>());
        let child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(), // Default linear curve
            "Child Profile".to_string(),
        );

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Parent's curve should be inherited (child has default)
        assert_eq!(merged.base_settings.filters.curve_points.len(), 3);
        assert_eq!(merged.base_settings.filters.curve_points[1].input, 0.5);
        assert_eq!(merged.base_settings.filters.curve_points[1].output, 0.7);
    }

    #[test]
    fn test_merge_with_parent_notch_filters_override() {
        // Create parent with notch filters
        let parent_id = must("parent-profile".parse::<ProfileId>());
        let mut parent = Profile::new(
            parent_id,
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.filters.notch_filters = vec![must(NotchFilter::new(
            must(FrequencyHz::new(60.0)),
            2.0,
            -12.0,
        ))];

        // Create child with different notch filters
        let child_id = must("child-profile".parse::<ProfileId>());
        let mut child = Profile::new_with_parent(
            child_id,
            parent.id.clone(),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.filters.notch_filters = vec![must(NotchFilter::new(
            must(FrequencyHz::new(120.0)),
            3.0,
            -6.0,
        ))];

        // Merge child with parent
        let merged = child.merge_with_parent(&parent);

        // Child's notch filters should override
        assert_eq!(merged.base_settings.filters.notch_filters.len(), 1);
        assert_eq!(
            merged.base_settings.filters.notch_filters[0]
                .frequency
                .value(),
            120.0
        );
    }

    #[test]
    fn test_is_approximately_equal() {
        assert!(Profile::is_approximately_equal(0.5, 0.5));
        assert!(Profile::is_approximately_equal(1.0, 1.0));
        assert!(!Profile::is_approximately_equal(0.5, 0.6));
        assert!(!Profile::is_approximately_equal(0.0, 0.1));
    }

    #[test]
    fn test_is_default_linear_curve() {
        let linear = vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(1.0, 1.0)),
        ];
        assert!(Profile::is_default_linear_curve(&linear));

        let non_linear = vec![
            must(CurvePoint::new(0.0, 0.0)),
            must(CurvePoint::new(0.5, 0.7)),
            must(CurvePoint::new(1.0, 1.0)),
        ];
        assert!(!Profile::is_default_linear_curve(&non_linear));

        let empty: Vec<CurvePoint> = vec![];
        assert!(!Profile::is_default_linear_curve(&empty));
    }

    // ==================== Inheritance Chain Resolution Tests ====================

    #[test]
    fn test_in_memory_profile_store_basic_operations() {
        let mut store = InMemoryProfileStore::new();
        assert!(store.is_empty());
        assert_eq!(store.len(), 0);

        let profile = Profile::new(
            must("test-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Test Profile".to_string(),
        );
        let profile_id = profile.id.clone();

        store.add(profile);
        assert!(!store.is_empty());
        assert_eq!(store.len(), 1);

        // Test get
        let retrieved = store.get(&profile_id);
        assert!(retrieved.is_some());
        assert_eq!(retrieved.map(|p| &p.id), Some(&profile_id));

        // Test get_mut
        let retrieved_mut = store.get_mut(&profile_id);
        assert!(retrieved_mut.is_some());

        // Test remove
        let removed = store.remove(&profile_id);
        assert!(removed.is_some());
        assert!(store.is_empty());
    }

    #[test]
    fn test_resolve_profile_without_parent() -> Result<(), DomainError> {
        let store = InMemoryProfileStore::new();

        let profile = Profile::new(
            must("root-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Root Profile".to_string(),
        );

        let resolved = profile.resolve(&store)?;

        // Should have only one profile in the chain
        assert_eq!(resolved.inheritance_chain.len(), 1);
        assert_eq!(resolved.inheritance_chain[0].as_str(), "root-profile");

        // Settings should match the original profile
        assert_eq!(
            resolved.effective_settings.ffb_gain.value(),
            profile.base_settings.ffb_gain.value()
        );

        Ok(())
    }

    #[test]
    fn test_resolve_single_level_inheritance() -> Result<(), DomainError> {
        let mut store = InMemoryProfileStore::new();

        // Create parent profile with custom settings
        let mut parent = Profile::new(
            must("parent-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.ffb_gain = must(Gain::new(0.8));
        parent.base_settings.degrees_of_rotation = must(Degrees::new_dor(540.0));
        store.add(parent.clone());

        // Create child profile that inherits from parent
        let mut child = Profile::new_with_parent(
            must("child-profile".parse::<ProfileId>()),
            must("parent-profile".parse::<ProfileId>()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.ffb_gain = must(Gain::new(0.6)); // Override FFB gain

        let resolved = child.resolve(&store)?;

        // Should have two profiles in the chain
        assert_eq!(resolved.inheritance_chain.len(), 2);
        assert_eq!(resolved.inheritance_chain[0].as_str(), "child-profile");
        assert_eq!(resolved.inheritance_chain[1].as_str(), "parent-profile");

        // Child's FFB gain should override parent's
        assert_eq!(resolved.effective_settings.ffb_gain.value(), 0.6);

        // Parent's DOR should be inherited (child has default)
        assert_eq!(
            resolved.effective_settings.degrees_of_rotation.value(),
            540.0
        );

        Ok(())
    }

    #[test]
    fn test_resolve_multi_level_inheritance() -> Result<(), DomainError> {
        let mut store = InMemoryProfileStore::new();

        // Create grandparent (root) profile
        let mut grandparent = Profile::new(
            must("grandparent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Grandparent Profile".to_string(),
        );
        grandparent.base_settings.ffb_gain = must(Gain::new(0.9));
        grandparent.base_settings.degrees_of_rotation = must(Degrees::new_dor(1080.0));
        grandparent.base_settings.torque_cap = must(TorqueNm::new(25.0));
        store.add(grandparent);

        // Create parent profile
        let mut parent = Profile::new_with_parent(
            must("parent".parse::<ProfileId>()),
            must("grandparent".parse::<ProfileId>()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.ffb_gain = must(Gain::new(0.8)); // Override FFB gain
        store.add(parent);

        // Create child profile
        let mut child = Profile::new_with_parent(
            must("child".parse::<ProfileId>()),
            must("parent".parse::<ProfileId>()),
            ProfileScope::for_car("iracing".to_string(), "gt3".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.degrees_of_rotation = must(Degrees::new_dor(540.0)); // Override DOR

        let resolved = child.resolve(&store)?;

        // Should have three profiles in the chain
        assert_eq!(resolved.inheritance_chain.len(), 3);
        assert_eq!(resolved.inheritance_chain[0].as_str(), "child");
        assert_eq!(resolved.inheritance_chain[1].as_str(), "parent");
        assert_eq!(resolved.inheritance_chain[2].as_str(), "grandparent");

        // Child's DOR should override
        assert_eq!(
            resolved.effective_settings.degrees_of_rotation.value(),
            540.0
        );

        // Parent's FFB gain should be inherited (child has default)
        assert_eq!(resolved.effective_settings.ffb_gain.value(), 0.8);

        // Grandparent's torque cap should be inherited
        assert_eq!(resolved.effective_settings.torque_cap.value(), 25.0);

        Ok(())
    }

    #[test]
    fn test_resolve_max_depth_exactly_5_levels() -> Result<(), DomainError> {
        let mut store = InMemoryProfileStore::new();

        // Create a chain of exactly 5 profiles (maximum allowed)
        let mut prev_id: Option<ProfileId> = None;

        for i in 0..5 {
            let id = must(format!("profile-{}", i).parse::<ProfileId>());
            let profile = if let Some(parent_id) = prev_id {
                Profile::new_with_parent(
                    id.clone(),
                    parent_id,
                    ProfileScope::global(),
                    BaseSettings::default(),
                    format!("Profile {}", i),
                )
            } else {
                Profile::new(
                    id.clone(),
                    ProfileScope::global(),
                    BaseSettings::default(),
                    format!("Profile {}", i),
                )
            };
            store.add(profile);
            prev_id = Some(id);
        }

        // Get the leaf profile (profile-4)
        let leaf_id = must("profile-4".parse::<ProfileId>());
        let leaf = store
            .get(&leaf_id)
            .ok_or_else(|| DomainError::ParentProfileNotFound {
                profile_id: "profile-4".to_string(),
            })?;

        // Resolution should succeed with exactly 5 levels
        let resolved = leaf.resolve(&store)?;
        assert_eq!(resolved.inheritance_chain.len(), 5);

        Ok(())
    }

    #[test]
    fn test_resolve_depth_exceeded_6_levels() {
        let mut store = InMemoryProfileStore::new();

        // Create a chain of 6 profiles (exceeds maximum of 5)
        let mut prev_id: Option<ProfileId> = None;

        for i in 0..6 {
            let id = must(format!("profile-{}", i).parse::<ProfileId>());
            let profile = if let Some(parent_id) = prev_id {
                Profile::new_with_parent(
                    id.clone(),
                    parent_id,
                    ProfileScope::global(),
                    BaseSettings::default(),
                    format!("Profile {}", i),
                )
            } else {
                Profile::new(
                    id.clone(),
                    ProfileScope::global(),
                    BaseSettings::default(),
                    format!("Profile {}", i),
                )
            };
            store.add(profile);
            prev_id = Some(id);
        }

        // Get the leaf profile (profile-5)
        let leaf_id = must("profile-5".parse::<ProfileId>());
        let leaf = store.get(&leaf_id);
        assert!(leaf.is_some());

        // Resolution should fail with depth exceeded error
        let result = leaf.map(|p| p.resolve(&store));
        assert!(result.is_some());

        let resolve_result = result.map(|r| r.err());
        assert!(matches!(
            resolve_result,
            Some(Some(DomainError::InheritanceDepthExceeded {
                depth: 6,
                max_depth: 5
            }))
        ));
    }

    #[test]
    fn test_resolve_circular_inheritance_direct() {
        let mut store = InMemoryProfileStore::new();

        // Create profile A that references profile B as parent
        let profile_a = Profile::new_with_parent(
            must("profile-a".parse::<ProfileId>()),
            must("profile-b".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Profile A".to_string(),
        );
        store.add(profile_a.clone());

        // Create profile B that references profile A as parent (circular!)
        let profile_b = Profile::new_with_parent(
            must("profile-b".parse::<ProfileId>()),
            must("profile-a".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Profile B".to_string(),
        );
        store.add(profile_b);

        // Resolution should fail with circular inheritance error
        let result = profile_a.resolve(&store);
        assert!(matches!(
            result,
            Err(DomainError::CircularInheritance { .. })
        ));
    }

    #[test]
    fn test_resolve_circular_inheritance_indirect() {
        let mut store = InMemoryProfileStore::new();

        // Create a chain: A -> B -> C -> A (circular)
        let profile_a = Profile::new_with_parent(
            must("profile-a".parse::<ProfileId>()),
            must("profile-b".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Profile A".to_string(),
        );
        store.add(profile_a.clone());

        let profile_b = Profile::new_with_parent(
            must("profile-b".parse::<ProfileId>()),
            must("profile-c".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Profile B".to_string(),
        );
        store.add(profile_b);

        let profile_c = Profile::new_with_parent(
            must("profile-c".parse::<ProfileId>()),
            must("profile-a".parse::<ProfileId>()), // Circular reference back to A
            ProfileScope::global(),
            BaseSettings::default(),
            "Profile C".to_string(),
        );
        store.add(profile_c);

        // Resolution should fail with circular inheritance error
        let result = profile_a.resolve(&store);
        assert!(matches!(
            result,
            Err(DomainError::CircularInheritance { .. })
        ));
    }

    #[test]
    fn test_resolve_parent_not_found() {
        let store = InMemoryProfileStore::new();

        // Create a profile with a parent that doesn't exist in the store
        let profile = Profile::new_with_parent(
            must("child-profile".parse::<ProfileId>()),
            must("nonexistent-parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );

        // Resolution should fail with parent not found error
        let result = profile.resolve(&store);
        assert!(matches!(
            result,
            Err(DomainError::ParentProfileNotFound { profile_id }) if profile_id == "nonexistent-parent"
        ));
    }

    #[test]
    fn test_resolve_led_config_inheritance() -> Result<(), DomainError> {
        let mut store = InMemoryProfileStore::new();

        // Create parent with LED config
        let mut parent = Profile::new(
            must("parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        let parent_led = LedConfig {
            brightness: must(Gain::new(0.9)),
            ..LedConfig::default()
        };
        parent.led_config = Some(parent_led);
        store.add(parent);

        // Create child without LED config
        let mut child = Profile::new_with_parent(
            must("child".parse::<ProfileId>()),
            must("parent".parse::<ProfileId>()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.led_config = None;

        let resolved = child.resolve(&store)?;

        // Parent's LED config should be inherited
        assert!(resolved.led_config.is_some());
        assert_eq!(
            resolved.led_config.as_ref().map(|l| l.brightness.value()),
            Some(0.9)
        );

        Ok(())
    }

    #[test]
    fn test_resolve_led_config_override() -> Result<(), DomainError> {
        let mut store = InMemoryProfileStore::new();

        // Create parent with LED config
        let mut parent = Profile::new(
            must("parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.led_config = Some(LedConfig::default());
        store.add(parent);

        // Create child with different LED config
        let mut child = Profile::new_with_parent(
            must("child".parse::<ProfileId>()),
            must("parent".parse::<ProfileId>()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        let child_led = LedConfig {
            brightness: must(Gain::new(0.5)),
            ..LedConfig::default()
        };
        child.led_config = Some(child_led);

        let resolved = child.resolve(&store)?;

        // Child's LED config should override
        assert!(resolved.led_config.is_some());
        assert_eq!(
            resolved.led_config.as_ref().map(|l| l.brightness.value()),
            Some(0.5)
        );

        Ok(())
    }

    #[test]
    fn test_validate_inheritance_success() -> Result<(), DomainError> {
        let mut store = InMemoryProfileStore::new();

        // Create a valid inheritance chain
        let parent = Profile::new(
            must("parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        store.add(parent);

        let child = Profile::new_with_parent(
            must("child".parse::<ProfileId>()),
            must("parent".parse::<ProfileId>()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );

        // Validation should succeed
        child.validate_inheritance(&store)?;

        Ok(())
    }

    #[test]
    fn test_validate_inheritance_circular_fails() {
        let mut store = InMemoryProfileStore::new();

        // Create circular inheritance
        let profile_a = Profile::new_with_parent(
            must("profile-a".parse::<ProfileId>()),
            must("profile-b".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Profile A".to_string(),
        );
        store.add(profile_a.clone());

        let profile_b = Profile::new_with_parent(
            must("profile-b".parse::<ProfileId>()),
            must("profile-a".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Profile B".to_string(),
        );
        store.add(profile_b);

        // Validation should fail
        let result = profile_a.validate_inheritance(&store);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_deterministic() -> Result<(), DomainError> {
        let mut store = InMemoryProfileStore::new();

        // Create parent profile
        let mut parent = Profile::new(
            must("parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        parent.base_settings.ffb_gain = must(Gain::new(0.8));
        store.add(parent);

        // Create child profile
        let mut child = Profile::new_with_parent(
            must("child".parse::<ProfileId>()),
            must("parent".parse::<ProfileId>()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child Profile".to_string(),
        );
        child.base_settings.degrees_of_rotation = must(Degrees::new_dor(540.0));

        // Resolve multiple times
        let resolved1 = child.resolve(&store)?;
        let resolved2 = child.resolve(&store)?;

        // Results should be identical
        assert_eq!(
            resolved1.effective_settings.ffb_gain.value(),
            resolved2.effective_settings.ffb_gain.value()
        );
        assert_eq!(
            resolved1.effective_settings.degrees_of_rotation.value(),
            resolved2.effective_settings.degrees_of_rotation.value()
        );
        assert_eq!(resolved1.inheritance_chain, resolved2.inheritance_chain);

        Ok(())
    }

    // ==================== Profile Change Notification Tests ====================

    use std::sync::{Arc, Mutex};

    /// Test observer that records all notifications it receives
    struct TestObserver {
        notifications: Mutex<Vec<(ProfileChangeEvent, Vec<ProfileId>)>>,
    }

    impl TestObserver {
        fn new() -> Self {
            Self {
                notifications: Mutex::new(Vec::new()),
            }
        }

        fn notification_count(&self) -> usize {
            match self.notifications.lock() {
                Ok(guard) => guard.len(),
                Err(poisoned) => poisoned.into_inner().len(),
            }
        }

        fn get_notifications(&self) -> Vec<(ProfileChangeEvent, Vec<ProfileId>)> {
            match self.notifications.lock() {
                Ok(guard) => guard.clone(),
                Err(poisoned) => poisoned.into_inner().clone(),
            }
        }
    }

    impl ProfileChangeObserver for TestObserver {
        fn on_profile_change(&self, event: &ProfileChangeEvent, affected_children: &[ProfileId]) {
            match self.notifications.lock() {
                Ok(mut guard) => {
                    guard.push((event.clone(), affected_children.to_vec()));
                }
                Err(mut poisoned) => {
                    poisoned
                        .get_mut()
                        .push((event.clone(), affected_children.to_vec()));
                }
            }
        }
    }

    #[test]
    fn test_register_observer() {
        let mut store = InMemoryProfileStore::new();
        assert_eq!(store.observer_count(), 0);

        let observer = Arc::new(TestObserver::new());
        store.register_observer(observer);
        assert_eq!(store.observer_count(), 1);
    }

    #[test]
    fn test_clear_observers() {
        let mut store = InMemoryProfileStore::new();

        let observer1 = Arc::new(TestObserver::new());
        let observer2 = Arc::new(TestObserver::new());
        store.register_observer(observer1);
        store.register_observer(observer2);
        assert_eq!(store.observer_count(), 2);

        store.clear_observers();
        assert_eq!(store.observer_count(), 0);
    }

    #[test]
    fn test_update_notifies_observers() {
        let mut store = InMemoryProfileStore::new();

        let observer = Arc::new(TestObserver::new());
        store.register_observer(observer.clone());

        // Add a profile using update (should notify)
        let profile = Profile::new(
            must("test-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Test Profile".to_string(),
        );
        store.update(profile);

        // Observer should have received one notification
        assert_eq!(observer.notification_count(), 1);

        let notifications = observer.get_notifications();
        assert!(matches!(
            &notifications[0].0,
            ProfileChangeEvent::Modified { profile_id } if profile_id.as_str() == "test-profile"
        ));
    }

    #[test]
    fn test_remove_notifies_observers() {
        let mut store = InMemoryProfileStore::new();

        // Add a profile first (without observer)
        let profile = Profile::new(
            must("test-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Test Profile".to_string(),
        );
        store.add(profile);

        // Now register observer
        let observer = Arc::new(TestObserver::new());
        store.register_observer(observer.clone());

        // Remove the profile
        let profile_id = must("test-profile".parse::<ProfileId>());
        let removed = store.remove(&profile_id);
        assert!(removed.is_some());

        // Observer should have received one notification
        assert_eq!(observer.notification_count(), 1);

        let notifications = observer.get_notifications();
        assert!(matches!(
            &notifications[0].0,
            ProfileChangeEvent::Removed { profile_id } if profile_id.as_str() == "test-profile"
        ));
    }

    #[test]
    fn test_update_includes_affected_children() {
        let mut store = InMemoryProfileStore::new();

        // Create parent profile
        let parent = Profile::new(
            must("parent-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent Profile".to_string(),
        );
        store.add(parent);

        // Create child profiles that inherit from parent
        let child1 = Profile::new_with_parent(
            must("child-1".parse::<ProfileId>()),
            must("parent-profile".parse::<ProfileId>()),
            ProfileScope::for_game("iracing".to_string()),
            BaseSettings::default(),
            "Child 1".to_string(),
        );
        store.add(child1);

        let child2 = Profile::new_with_parent(
            must("child-2".parse::<ProfileId>()),
            must("parent-profile".parse::<ProfileId>()),
            ProfileScope::for_game("acc".to_string()),
            BaseSettings::default(),
            "Child 2".to_string(),
        );
        store.add(child2);

        // Register observer
        let observer = Arc::new(TestObserver::new());
        store.register_observer(observer.clone());

        // Update the parent profile
        let mut updated_parent = Profile::new(
            must("parent-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Updated Parent Profile".to_string(),
        );
        updated_parent.base_settings.ffb_gain = must(Gain::new(0.9));
        store.update(updated_parent);

        // Observer should have received notification with affected children
        assert_eq!(observer.notification_count(), 1);

        let notifications = observer.get_notifications();
        let (event, affected_children) = &notifications[0];

        assert!(matches!(
            event,
            ProfileChangeEvent::Modified { profile_id } if profile_id.as_str() == "parent-profile"
        ));

        // Both children should be in the affected list
        assert_eq!(affected_children.len(), 2);
        let child_ids: Vec<&str> = affected_children.iter().map(|id| id.as_str()).collect();
        assert!(child_ids.contains(&"child-1"));
        assert!(child_ids.contains(&"child-2"));
    }

    #[test]
    fn test_find_children() {
        let mut store = InMemoryProfileStore::new();

        // Create parent profile
        let parent = Profile::new(
            must("parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent".to_string(),
        );
        store.add(parent);

        // Create child profiles
        let child1 = Profile::new_with_parent(
            must("child-1".parse::<ProfileId>()),
            must("parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Child 1".to_string(),
        );
        store.add(child1);

        let child2 = Profile::new_with_parent(
            must("child-2".parse::<ProfileId>()),
            must("parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Child 2".to_string(),
        );
        store.add(child2);

        // Create unrelated profile
        let unrelated = Profile::new(
            must("unrelated".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Unrelated".to_string(),
        );
        store.add(unrelated);

        // Find children of parent
        let parent_id = must("parent".parse::<ProfileId>());
        let children = store.find_children(&parent_id);

        assert_eq!(children.len(), 2);
        let child_ids: Vec<&str> = children.iter().map(|id| id.as_str()).collect();
        assert!(child_ids.contains(&"child-1"));
        assert!(child_ids.contains(&"child-2"));
        assert!(!child_ids.contains(&"unrelated"));
    }

    #[test]
    fn test_find_all_descendants() {
        let mut store = InMemoryProfileStore::new();

        // Create a hierarchy: grandparent -> parent -> child
        let grandparent = Profile::new(
            must("grandparent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Grandparent".to_string(),
        );
        store.add(grandparent);

        let parent = Profile::new_with_parent(
            must("parent".parse::<ProfileId>()),
            must("grandparent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Parent".to_string(),
        );
        store.add(parent);

        let child = Profile::new_with_parent(
            must("child".parse::<ProfileId>()),
            must("parent".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Child".to_string(),
        );
        store.add(child);

        // Find all descendants of grandparent
        let grandparent_id = must("grandparent".parse::<ProfileId>());
        let descendants = store.find_all_descendants(&grandparent_id);

        assert_eq!(descendants.len(), 2);
        let descendant_ids: Vec<&str> = descendants.iter().map(|id| id.as_str()).collect();
        assert!(descendant_ids.contains(&"parent"));
        assert!(descendant_ids.contains(&"child"));
    }

    #[test]
    fn test_multiple_observers_all_notified() {
        let mut store = InMemoryProfileStore::new();

        let observer1 = Arc::new(TestObserver::new());
        let observer2 = Arc::new(TestObserver::new());
        store.register_observer(observer1.clone());
        store.register_observer(observer2.clone());

        // Update a profile
        let profile = Profile::new(
            must("test-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Test Profile".to_string(),
        );
        store.update(profile);

        // Both observers should have received the notification
        assert_eq!(observer1.notification_count(), 1);
        assert_eq!(observer2.notification_count(), 1);
    }

    #[test]
    fn test_add_does_not_notify_observers() {
        let mut store = InMemoryProfileStore::new();

        let observer = Arc::new(TestObserver::new());
        store.register_observer(observer.clone());

        // Add a profile using add() (should NOT notify - use update() for notifications)
        let profile = Profile::new(
            must("test-profile".parse::<ProfileId>()),
            ProfileScope::global(),
            BaseSettings::default(),
            "Test Profile".to_string(),
        );
        store.add(profile);

        // Observer should NOT have received any notification
        // add() is for initial population, update() is for changes
        assert_eq!(observer.notification_count(), 0);
    }

    #[test]
    fn test_remove_nonexistent_does_not_notify() {
        let mut store = InMemoryProfileStore::new();

        let observer = Arc::new(TestObserver::new());
        store.register_observer(observer.clone());

        // Try to remove a profile that doesn't exist
        let profile_id = must("nonexistent".parse::<ProfileId>());
        let removed = store.remove(&profile_id);
        assert!(removed.is_none());

        // Observer should NOT have received any notification
        assert_eq!(observer.notification_count(), 0);
    }

    #[test]
    fn test_profile_change_event_equality() {
        let id1 = must("profile-1".parse::<ProfileId>());
        let id2 = must("profile-2".parse::<ProfileId>());

        let event1 = ProfileChangeEvent::Modified {
            profile_id: id1.clone(),
        };
        let event2 = ProfileChangeEvent::Modified {
            profile_id: id1.clone(),
        };
        let event3 = ProfileChangeEvent::Modified {
            profile_id: id2.clone(),
        };
        let event4 = ProfileChangeEvent::Removed {
            profile_id: id1.clone(),
        };

        assert_eq!(event1, event2);
        assert_ne!(event1, event3);
        assert_ne!(event1, event4);
    }
}
