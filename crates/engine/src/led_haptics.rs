//! LED and Haptics Output System
//!
//! This module provides rate-independent LED and haptics output that operates
//! separately from the 1kHz FFB loop to ensure no interference with real-time
//! force feedback performance.

use crate::ports::{NormalizedTelemetry, TelemetryFlags};
use crate::prelude::MutexExt;
use racing_wheel_schemas::prelude::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio::time::interval;

/// LED pattern types for different racing scenarios
#[derive(Debug, Clone, PartialEq)]
pub enum LedPattern {
    /// RPM-based pattern with configurable bands
    RpmBands {
        bands: Vec<f32>,
        colors: Vec<LedColor>,
        hysteresis_percent: f32,
    },
    /// Flag-based patterns
    Flag {
        flag_type: FlagType,
        pattern: FlagPattern,
    },
    /// Pit limiter indication
    PitLimiter { blink_rate_hz: f32, color: LedColor },
    /// Launch control indication
    LaunchControl { pulse_rate_hz: f32, color: LedColor },
    /// Solid color
    Solid(LedColor),
    /// Off
    Off,
}

/// LED color representation
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LedColor {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl LedColor {
    pub const RED: LedColor = LedColor { r: 255, g: 0, b: 0 };
    pub const GREEN: LedColor = LedColor { r: 0, g: 255, b: 0 };
    pub const BLUE: LedColor = LedColor { r: 0, g: 0, b: 255 };
    pub const YELLOW: LedColor = LedColor {
        r: 255,
        g: 255,
        b: 0,
    };
    pub const WHITE: LedColor = LedColor {
        r: 255,
        g: 255,
        b: 255,
    };
    pub const ORANGE: LedColor = LedColor {
        r: 255,
        g: 165,
        b: 0,
    };
    pub const OFF: LedColor = LedColor { r: 0, g: 0, b: 0 };
}

/// Racing flag types
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlagType {
    Yellow,
    Red,
    Blue,
    Checkered,
    Green,
}

/// Flag display patterns
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum FlagPattern {
    Solid,
    Blink,
    Wipe,
    Flash,
}
// Haptics feedback types
#[derive(Debug, Clone, PartialEq)]
pub enum HapticsPattern {
    /// Rim vibration for road texture, kerbs, etc.
    RimVibration { intensity: f32, frequency_hz: f32 },
    /// Pedal feedback for ABS, traction control
    PedalFeedback {
        pedal: PedalType,
        intensity: f32,
        frequency_hz: f32,
    },
    /// Engine vibration based on RPM
    EngineVibration {
        base_frequency: f32,
        rpm_multiplier: f32,
        intensity: f32,
    },
    /// Off
    Off,
}

/// Pedal types for haptic feedback
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PedalType {
    Brake,
    Throttle,
    Clutch,
}

/// Dash widget types
#[derive(Debug, Clone, PartialEq)]
pub enum DashWidget {
    /// Gear display
    Gear {
        current_gear: i8,
        suggested_gear: Option<i8>,
    },
    /// RPM display with redline
    Rpm {
        current_rpm: f32,
        max_rpm: f32,
        redline_rpm: f32,
    },
    /// Speed display
    Speed { speed_kmh: f32, unit: SpeedUnit },
    /// Delta time display
    Delta {
        delta_seconds: f32,
        is_positive: bool,
    },
    /// Flag status
    Flags { active_flags: Vec<FlagType> },
    /// DRS status
    Drs { available: bool, enabled: bool },
    /// ERS status
    Ers {
        available: bool,
        deployment_percent: f32,
    },
}

/// Speed unit for display
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SpeedUnit {
    Kmh,
    Mph,
}
// LED mapping engine that generates patterns based on telemetry
pub struct LedMappingEngine {
    config: LedConfig,
    current_pattern: LedPattern,
    rpm_hysteresis_state: RpmHysteresisState,
    last_update: Instant,
    pattern_cache: HashMap<String, Vec<LedColor>>,
}

/// RPM hysteresis state to prevent flickering
#[derive(Debug, Clone)]
struct RpmHysteresisState {
    current_band: usize,
    last_rpm: f32,
    hysteresis_percent: f32,
}

impl LedMappingEngine {
    /// Create a new LED mapping engine
    pub fn new(config: LedConfig) -> Self {
        Self {
            config,
            current_pattern: LedPattern::Off,
            rpm_hysteresis_state: RpmHysteresisState {
                current_band: 0,
                last_rpm: 0.0,
                hysteresis_percent: 0.05, // 5% default hysteresis
            },
            last_update: Instant::now(),
            pattern_cache: HashMap::new(),
        }
    }

    /// Update LED pattern based on telemetry
    pub fn update_pattern(&mut self, telemetry: &NormalizedTelemetry) -> Vec<LedColor> {
        let now = Instant::now();

        // Determine priority pattern based on telemetry
        let new_pattern = self.determine_pattern(telemetry, now);

        // Generate LED colors for the pattern
        let colors = self.generate_colors(&new_pattern, now);

        self.current_pattern = new_pattern;
        self.last_update = now;

        colors
    }

    /// Determine the appropriate LED pattern based on telemetry priority
    fn determine_pattern(&mut self, telemetry: &NormalizedTelemetry, _now: Instant) -> LedPattern {
        // Priority order: Flags > Pit Limiter > RPM bands

        // Check for active flags (highest priority)
        if let Some(flag_pattern) = self.check_flag_patterns(&telemetry.flags) {
            return flag_pattern;
        }

        // Check for pit limiter
        if telemetry.flags.pit_limiter {
            return LedPattern::PitLimiter {
                blink_rate_hz: 2.0,
                color: LedColor::BLUE,
            };
        }

        // Default to RPM-based pattern
        self.generate_rpm_pattern(telemetry.rpm, telemetry.speed_ms)
    }

    /// Check for active racing flags and return appropriate pattern
    fn check_flag_patterns(&self, flags: &TelemetryFlags) -> Option<LedPattern> {
        if flags.red_flag {
            Some(LedPattern::Flag {
                flag_type: FlagType::Red,
                pattern: FlagPattern::Solid,
            })
        } else if flags.yellow_flag {
            Some(LedPattern::Flag {
                flag_type: FlagType::Yellow,
                pattern: FlagPattern::Blink,
            })
        } else if flags.blue_flag {
            Some(LedPattern::Flag {
                flag_type: FlagType::Blue,
                pattern: FlagPattern::Wipe,
            })
        } else if flags.checkered_flag {
            Some(LedPattern::Flag {
                flag_type: FlagType::Checkered,
                pattern: FlagPattern::Flash,
            })
        } else {
            None
        }
    }

    /// Generate RPM-based pattern with hysteresis
    fn generate_rpm_pattern(&mut self, rpm: f32, _speed_ms: f32) -> LedPattern {
        // Update hysteresis state
        self.update_rpm_hysteresis(rpm);

        // Get RPM bands from config
        let bands = vec![0.75, 0.82, 0.88, 0.92, 0.96]; // Default bands as percentages
        let colors = vec![
            LedColor::GREEN,
            LedColor::YELLOW,
            LedColor::ORANGE,
            LedColor::RED,
            LedColor::WHITE,
        ];

        LedPattern::RpmBands {
            bands,
            colors,
            hysteresis_percent: self.rpm_hysteresis_state.hysteresis_percent,
        }
    }

    /// Update RPM hysteresis state to prevent flickering
    fn update_rpm_hysteresis(&mut self, rpm: f32) {
        let hysteresis = self.rpm_hysteresis_state.hysteresis_percent;
        let bands = [0.75, 0.82, 0.88, 0.92, 0.96]; // Should come from config

        // Normalize RPM (assuming max RPM is available in telemetry)
        let max_rpm = 8000.0; // Should come from car/engine data
        let normalized_rpm = (rpm / max_rpm).clamp(0.0, 1.0);

        // Apply hysteresis logic
        let current_band = self.rpm_hysteresis_state.current_band;

        // Check if we should move to a higher band
        if current_band < bands.len() - 1 {
            let next_threshold = bands[current_band + 1];
            if normalized_rpm >= next_threshold {
                self.rpm_hysteresis_state.current_band = current_band + 1;
            }
        }

        // Check if we should move to a lower band (with hysteresis)
        if current_band > 0 {
            let current_threshold = bands[current_band];
            let hysteresis_threshold = current_threshold * (1.0 - hysteresis);
            if normalized_rpm < hysteresis_threshold {
                self.rpm_hysteresis_state.current_band = current_band - 1;
            }
        }

        self.rpm_hysteresis_state.last_rpm = normalized_rpm;
    }

    /// Generate actual LED colors for a pattern
    fn generate_colors(&self, pattern: &LedPattern, now: Instant) -> Vec<LedColor> {
        const LED_COUNT: usize = 16; // Typical wheel LED count
        let mut colors = vec![LedColor::OFF; LED_COUNT];

        match pattern {
            LedPattern::RpmBands {
                bands,
                colors: band_colors,
                ..
            } => {
                let current_band = self.rpm_hysteresis_state.current_band;
                let leds_to_light = ((current_band + 1) * LED_COUNT / bands.len()).min(LED_COUNT);

                #[allow(clippy::needless_range_loop)]
                for i in 0..leds_to_light {
                    let band_index = (i * bands.len() / LED_COUNT).min(band_colors.len() - 1);
                    colors[i] = band_colors[band_index];
                }
            }

            LedPattern::Flag {
                flag_type,
                pattern: flag_pattern,
            } => {
                let color = match flag_type {
                    FlagType::Yellow => LedColor::YELLOW,
                    FlagType::Red => LedColor::RED,
                    FlagType::Blue => LedColor::BLUE,
                    FlagType::Checkered => LedColor::WHITE,
                    FlagType::Green => LedColor::GREEN,
                };

                match flag_pattern {
                    FlagPattern::Solid => {
                        colors.fill(color);
                    }
                    FlagPattern::Blink => {
                        let blink_on = (now.elapsed().as_millis() / 500).is_multiple_of(2);
                        if blink_on {
                            colors.fill(color);
                        }
                    }
                    FlagPattern::Wipe => {
                        let cycle_ms = 1000;
                        let position =
                            (now.elapsed().as_millis() % cycle_ms) as f32 / cycle_ms as f32;
                        let led_position = (position * LED_COUNT as f32) as usize;

                        #[allow(clippy::needless_range_loop)]
                        for i in 0..led_position.min(LED_COUNT - 1) {
                            colors[i] = color;
                        }
                    }
                    FlagPattern::Flash => {
                        let flash_on = (now.elapsed().as_millis() / 100).is_multiple_of(2);
                        if flash_on {
                            colors.fill(color);
                        }
                    }
                }
            }

            LedPattern::PitLimiter {
                blink_rate_hz,
                color,
            } => {
                let period_ms = (1000.0 / blink_rate_hz) as u128;
                let blink_on = (now.elapsed().as_millis() / period_ms).is_multiple_of(2);
                if blink_on {
                    colors.fill(*color);
                }
            }

            LedPattern::LaunchControl {
                pulse_rate_hz,
                color,
            } => {
                let period_ms = (1000.0 / pulse_rate_hz) as u128;
                let pulse_phase = (now.elapsed().as_millis() % period_ms) as f32 / period_ms as f32;
                let intensity = (pulse_phase * std::f32::consts::PI * 2.0).sin().abs();

                let dimmed_color = LedColor {
                    r: (color.r as f32 * intensity) as u8,
                    g: (color.g as f32 * intensity) as u8,
                    b: (color.b as f32 * intensity) as u8,
                };
                colors.fill(dimmed_color);
            }

            LedPattern::Solid(color) => {
                colors.fill(*color);
            }

            LedPattern::Off => {
                // Colors already initialized to OFF
            }
        }

        colors
    }

    /// Get current pattern for testing/debugging
    pub fn current_pattern(&self) -> &LedPattern {
        &self.current_pattern
    }

    /// Update configuration
    pub fn update_config(&mut self, config: LedConfig) {
        self.config = config;
        self.pattern_cache.clear(); // Clear cache when config changes
    }
}

/// Haptics routing system for rim and pedal feedback
pub struct HapticsRouter {
    config: HapticsConfig,
    active_patterns: HashMap<String, HapticsPattern>,
    last_update: Instant,
}

impl HapticsRouter {
    /// Create a new haptics router
    pub fn new(config: HapticsConfig) -> Self {
        Self {
            config,
            active_patterns: HashMap::new(),
            last_update: Instant::now(),
        }
    }

    /// Update haptics patterns based on telemetry
    pub fn update_patterns(
        &mut self,
        telemetry: &NormalizedTelemetry,
    ) -> HashMap<String, HapticsPattern> {
        let now = Instant::now();

        // Clear previous patterns
        self.active_patterns.clear();

        // Generate rim vibration based on speed and slip
        if telemetry.slip_ratio > 0.1 {
            let intensity = (telemetry.slip_ratio * 0.8).clamp(0.0, 1.0);
            self.active_patterns.insert(
                "rim_slip".to_string(),
                HapticsPattern::RimVibration {
                    intensity,
                    frequency_hz: 25.0 + telemetry.slip_ratio * 50.0,
                },
            );
        }

        // Generate engine vibration based on RPM
        if telemetry.rpm > 1000.0 {
            let intensity = (telemetry.rpm / 8000.0 * 0.3).clamp(0.0, 0.5);
            self.active_patterns.insert(
                "engine".to_string(),
                HapticsPattern::EngineVibration {
                    base_frequency: 20.0,
                    rpm_multiplier: telemetry.rpm / 1000.0,
                    intensity,
                },
            );
        }

        // Generate pedal feedback for ABS/TC (simulated based on slip)
        if telemetry.slip_ratio > 0.3 {
            self.active_patterns.insert(
                "brake_abs".to_string(),
                HapticsPattern::PedalFeedback {
                    pedal: PedalType::Brake,
                    intensity: 0.7,
                    frequency_hz: 15.0,
                },
            );
        }

        self.last_update = now;
        self.active_patterns.clone()
    }

    /// Update configuration
    pub fn update_config(&mut self, config: HapticsConfig) {
        self.config = config;
    }

    /// Get active patterns for testing
    pub fn active_patterns(&self) -> &HashMap<String, HapticsPattern> {
        &self.active_patterns
    }
}

/// Dash widget system for displaying racing information
pub struct DashWidgetSystem {
    widgets: HashMap<String, DashWidget>,
    last_update: Instant,
}

impl DashWidgetSystem {
    /// Create a new dash widget system
    pub fn new() -> Self {
        Self {
            widgets: HashMap::new(),
            last_update: Instant::now(),
        }
    }

    /// Update all widgets based on telemetry
    pub fn update_widgets(
        &mut self,
        telemetry: &NormalizedTelemetry,
    ) -> HashMap<String, DashWidget> {
        let now = Instant::now();

        // Update gear widget
        self.widgets.insert(
            "gear".to_string(),
            DashWidget::Gear {
                current_gear: telemetry.gear,
                suggested_gear: None, // Could be calculated based on RPM/speed
            },
        );

        // Update RPM widget
        self.widgets.insert(
            "rpm".to_string(),
            DashWidget::Rpm {
                current_rpm: telemetry.rpm,
                max_rpm: 8000.0,     // Should come from car data
                redline_rpm: 7500.0, // Should come from car data
            },
        );

        // Update speed widget
        let speed_kmh = telemetry.speed_ms * 3.6;
        self.widgets.insert(
            "speed".to_string(),
            DashWidget::Speed {
                speed_kmh,
                unit: SpeedUnit::Kmh,
            },
        );

        // Update flags widget
        let mut active_flags = Vec::new();
        if telemetry.flags.yellow_flag {
            active_flags.push(FlagType::Yellow);
        }
        if telemetry.flags.red_flag {
            active_flags.push(FlagType::Red);
        }
        if telemetry.flags.blue_flag {
            active_flags.push(FlagType::Blue);
        }
        if telemetry.flags.checkered_flag {
            active_flags.push(FlagType::Checkered);
        }

        self.widgets
            .insert("flags".to_string(), DashWidget::Flags { active_flags });

        // Update DRS widget
        self.widgets.insert(
            "drs".to_string(),
            DashWidget::Drs {
                available: telemetry.flags.drs_enabled,
                enabled: telemetry.flags.drs_enabled,
            },
        );

        // Update ERS widget
        self.widgets.insert(
            "ers".to_string(),
            DashWidget::Ers {
                available: telemetry.flags.ers_available,
                deployment_percent: 0.0, // Would need additional telemetry data
            },
        );

        self.last_update = now;
        self.widgets.clone()
    }

    /// Get current widgets for testing
    pub fn widgets(&self) -> &HashMap<String, DashWidget> {
        &self.widgets
    }
}

impl Default for DashWidgetSystem {
    fn default() -> Self {
        Self::new()
    }
}

/// Rate-independent LED and haptics output system
///
/// This system operates at 60-200Hz independently of the 1kHz FFB loop
/// to ensure no interference with real-time force feedback performance.
pub struct LedHapticsSystem {
    device_id: DeviceId,
    led_engine: Arc<Mutex<LedMappingEngine>>,
    haptics_router: Arc<Mutex<HapticsRouter>>,
    dash_widgets: Arc<Mutex<DashWidgetSystem>>,
    telemetry_rx: Option<mpsc::Receiver<NormalizedTelemetry>>,
    output_tx: mpsc::Sender<LedHapticsOutput>,
    is_running: Arc<Mutex<bool>>,
    update_rate_hz: f32,
}

/// Combined output for LEDs and haptics
#[derive(Debug, Clone)]
pub struct LedHapticsOutput {
    pub device_id: DeviceId,
    pub led_colors: Vec<LedColor>,
    pub haptics_patterns: HashMap<String, HapticsPattern>,
    pub dash_widgets: HashMap<String, DashWidget>,
    pub timestamp: Instant,
}

impl LedHapticsSystem {
    /// Create a new LED and haptics system
    pub fn new(
        device_id: DeviceId,
        led_config: LedConfig,
        haptics_config: HapticsConfig,
        update_rate_hz: f32,
    ) -> (Self, mpsc::Receiver<LedHapticsOutput>) {
        let (output_tx, output_rx) = mpsc::channel(100);

        let system = Self {
            device_id: device_id.clone(),
            led_engine: Arc::new(Mutex::new(LedMappingEngine::new(led_config))),
            haptics_router: Arc::new(Mutex::new(HapticsRouter::new(haptics_config))),
            dash_widgets: Arc::new(Mutex::new(DashWidgetSystem::new())),
            telemetry_rx: None,
            output_tx,
            is_running: Arc::new(Mutex::new(false)),
            update_rate_hz: update_rate_hz.clamp(60.0, 200.0),
        };

        (system, output_rx)
    }

    /// Start the LED and haptics processing loop
    pub async fn start(
        &mut self,
        telemetry_rx: mpsc::Receiver<NormalizedTelemetry>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        self.telemetry_rx = Some(telemetry_rx);

        {
            let mut running = self.is_running.lock_or_panic();
            *running = true;
        }

        let led_engine = Arc::clone(&self.led_engine);
        let haptics_router = Arc::clone(&self.haptics_router);
        let dash_widgets = Arc::clone(&self.dash_widgets);
        let output_tx = self.output_tx.clone();
        let device_id = self.device_id.clone();
        let is_running = Arc::clone(&self.is_running);
        let update_rate_hz = self.update_rate_hz;

        let mut telemetry_rx = self.telemetry_rx.take().ok_or("telemetry receiver already taken")?;

        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs_f32(1.0 / update_rate_hz));
            let mut last_telemetry: Option<NormalizedTelemetry> = None;

            loop {
                // Check if we should continue running
                {
                    let running = is_running.lock_or_panic();
                    if !*running {
                        break;
                    }
                }

                // Try to get latest telemetry (non-blocking)
                while let Ok(telemetry) = telemetry_rx.try_recv() {
                    last_telemetry = Some(telemetry);
                }

                // Process output if we have telemetry
                if let Some(ref telemetry) = last_telemetry {
                    let led_colors = {
                        let mut engine = led_engine.lock_or_panic();
                        engine.update_pattern(telemetry)
                    };

                    let haptics_patterns = {
                        let mut router = haptics_router.lock_or_panic();
                        router.update_patterns(telemetry)
                    };

                    let widgets = {
                        let mut dash = dash_widgets.lock_or_panic();
                        dash.update_widgets(telemetry)
                    };

                    let output = LedHapticsOutput {
                        device_id: device_id.clone(),
                        led_colors,
                        haptics_patterns,
                        dash_widgets: widgets,
                        timestamp: Instant::now(),
                    };

                    // Send output (non-blocking)
                    if output_tx.try_send(output).is_err() {
                        // Output channel full, drop frame
                        // This is acceptable for LED/haptics output
                    }
                }

                interval.tick().await;
            }
        });

        Ok(())
    }

    /// Stop the LED and haptics processing
    pub fn stop(&self) {
        let mut running = self.is_running.lock_or_panic();
        *running = false;
    }

    /// Update LED configuration
    pub fn update_led_config(&self, config: LedConfig) {
        let mut engine = self.led_engine.lock_or_panic();
        engine.update_config(config);
    }

    /// Update haptics configuration
    pub fn update_haptics_config(&self, config: HapticsConfig) {
        let mut router = self.haptics_router.lock_or_panic();
        router.update_config(config);
    }

    /// Check if system is running
    pub fn is_running(&self) -> bool {
        *self.is_running.lock_or_panic()
    }
}
