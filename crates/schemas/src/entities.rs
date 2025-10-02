//! Domain entities for the racing wheel software
//!
//! This module contains the core domain entities that represent the main
//! business objects in the system. These are pure domain objects with
//! no dependencies on infrastructure concerns.

use crate::domain::{
    DeviceId, ProfileId, TorqueNm, Degrees, Gain, FrequencyHz, CurvePoint, 
    DomainError, validate_curve_monotonic
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
        matches!(self.state, DeviceState::Active | DeviceState::SafeMode)
            && self.fault_flags == 0
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
            start_angle: 450.0,  // Start at 450 degrees from center
            max_angle: 540.0,    // Hard stop at 540 degrees
            stiffness: 0.8,
            damping: 0.3,
        }
    }
}

impl Default for HandsOffConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            threshold: 0.05,     // 5% torque threshold
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
            friction: Gain::from_raw(0.0),
            damper: Gain::from_raw(0.0),
            inertia: Gain::from_raw(0.0),
            notch_filters: Vec::new(),
            slew_rate: Gain::from_raw(1.0), // No slew rate limiting
            curve_points: vec![
                CurvePoint::new(0.0, 0.0).unwrap(),
                CurvePoint::new(1.0, 1.0).unwrap(),
            ],
            torque_cap: Gain::from_raw(1.0), // No torque cap by default
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
            return Err(DomainError::InvalidCurvePoints(
                format!("Reconstruction level must be 0-8, got {}", reconstruction)
            ));
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
            torque_cap: Gain::from_raw(1.0), // No cap by default
            bumpstop: BumpstopConfig::default(),
            hands_off: HandsOffConfig::default(),
        })
    }

    /// Create a new filter configuration with all parameters
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
            return Err(DomainError::InvalidCurvePoints(
                format!("Reconstruction level must be 0-8, got {}", reconstruction)
            ));
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
    pub fn validate_for_device(&self, capabilities: &DeviceCapabilities) -> Result<(), DomainError> {
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
            ffb_gain: Gain::from_raw(0.7),
            degrees_of_rotation: Degrees::from_raw(900.0),
            torque_cap: TorqueNm::from_raw(15.0),
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
            if band < 0.0 || band > 1.0 {
                return Err(DomainError::InvalidCurvePoints(
                    format!("RPM band must be 0.0-1.0, got {}", band)
                ));
            }
        }
        
        // Check if bands are sorted
        for window in rpm_bands.windows(2) {
            if window[1] <= window[0] {
                return Err(DomainError::InvalidCurvePoints(
                    "RPM bands must be in ascending order".to_string()
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
            brightness: Gain::from_raw(0.8),
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
            intensity: Gain::from_raw(0.6),
            frequency: FrequencyHz::from_raw(80.0),
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
        if self.game.is_some() { level += 1; }
        if self.car.is_some() { level += 1; }
        if self.track.is_some() { level += 1; }
        level
    }
    
    /// Check if this scope matches a given context
    pub fn matches(&self, game: Option<&str>, car: Option<&str>, track: Option<&str>) -> bool {
        // Check game match
        if let Some(ref scope_game) = self.game {
            if game != Some(scope_game.as_str()) {
                return false;
            }
        }
        
        // Check car match
        if let Some(ref scope_car) = self.car {
            if car != Some(scope_car.as_str()) {
                return false;
            }
        }
        
        // Check track match
        if let Some(ref scope_track) = self.track {
            if track != Some(scope_track.as_str()) {
                return false;
            }
        }
        
        true
    }
}

/// Complete profile configuration
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    /// Unique profile identifier
    pub id: ProfileId,
    
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
    
    /// Validate profile against device capabilities
    pub fn validate_for_device(&self, capabilities: &DeviceCapabilities) -> Result<(), DomainError> {
        self.base_settings.validate_for_device(capabilities)
    }
    
    /// Calculate a hash for deterministic profile comparison
    pub fn calculate_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        
        let mut hasher = DefaultHasher::new();
        
        // Hash the settings that affect behavior (not metadata)
        self.base_settings.ffb_gain.value().to_bits().hash(&mut hasher);
        self.base_settings.degrees_of_rotation.value().to_bits().hash(&mut hasher);
        self.base_settings.torque_cap.value().to_bits().hash(&mut hasher);
        
        // Hash filter config
        self.base_settings.filters.reconstruction.hash(&mut hasher);
        self.base_settings.filters.friction.value().to_bits().hash(&mut hasher);
        self.base_settings.filters.damper.value().to_bits().hash(&mut hasher);
        self.base_settings.filters.inertia.value().to_bits().hash(&mut hasher);
        self.base_settings.filters.slew_rate.value().to_bits().hash(&mut hasher);
        
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_capabilities() {
        let caps = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            TorqueNm::new(25.0).unwrap(),
            10000,
            1000,
        );
        
        assert!(caps.supports_ffb());
        assert_eq!(caps.max_update_rate_hz(), 1000.0);
    }

    #[test]
    fn test_device_creation() {
        let id: DeviceId = "test-device".parse().unwrap();
        let caps = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            TorqueNm::new(25.0).unwrap(),
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
        let id: DeviceId = "test-device".parse().unwrap();
        let caps = DeviceCapabilities::new(
            false,
            true,
            true,
            true,
            TorqueNm::new(25.0).unwrap(),
            10000,
            1000,
        );
        
        let mut device = Device::new(
            id,
            "Test Wheel".to_string(),
            DeviceType::WheelBase,
            caps,
        );
        
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
            Gain::new(0.1).unwrap(),
            Gain::new(0.15).unwrap(),
            Gain::new(0.05).unwrap(),
            vec![],
            Gain::new(0.8).unwrap(),
            vec![
                CurvePoint::new(0.0, 0.0).unwrap(),
                CurvePoint::new(1.0, 1.0).unwrap(),
            ],
        );
        assert!(config.is_ok());
        
        // Invalid reconstruction level
        let bad_config = FilterConfig::new(
            10, // Too high
            Gain::new(0.1).unwrap(),
            Gain::new(0.15).unwrap(),
            Gain::new(0.05).unwrap(),
            vec![],
            Gain::new(0.8).unwrap(),
            vec![
                CurvePoint::new(0.0, 0.0).unwrap(),
                CurvePoint::new(1.0, 1.0).unwrap(),
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
        let track_scope = ProfileScope::for_track(
            "iracing".to_string(),
            "gt3".to_string(),
            "spa".to_string(),
        );
        
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
        let id: ProfileId = "test-profile".parse().unwrap();
        let scope = ProfileScope::for_game("iracing".to_string());
        let base_settings = BaseSettings::default();
        
        let profile = Profile::new(
            id.clone(),
            scope,
            base_settings,
            "Test Profile".to_string(),
        );
        
        assert_eq!(profile.id, id);
        assert_eq!(profile.metadata.name, "Test Profile");
        assert!(profile.led_config.is_some());
        assert!(profile.haptics_config.is_some());
    }

    #[test]
    fn test_profile_hash_deterministic() {
        let id: ProfileId = "test-profile".parse().unwrap();
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
            Gain::new(0.8).unwrap(),
            HashMap::new(),
        );
        assert!(config.is_ok());
        
        // Invalid RPM bands (not sorted)
        let bad_config = LedConfig::new(
            vec![0.75, 0.92, 0.82], // Not sorted
            "progressive".to_string(),
            Gain::new(0.8).unwrap(),
            HashMap::new(),
        );
        assert!(bad_config.is_err());
        
        // Invalid RPM bands (out of range)
        let bad_config2 = LedConfig::new(
            vec![0.75, 1.2], // > 1.0
            "progressive".to_string(),
            Gain::new(0.8).unwrap(),
            HashMap::new(),
        );
        assert!(bad_config2.is_err());
    }

    #[test]
    fn test_notch_filter_validation() {
        let freq = FrequencyHz::new(60.0).unwrap();
        
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
}