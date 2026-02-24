//! Adaptive scheduling configuration and state.

/// Adaptive scheduling configuration.
///
/// Adaptive scheduling dynamically adjusts the scheduler period within bounded
/// limits to reduce timing violations under load, then returns toward the base
/// period when the system is healthy.
#[derive(Debug, Clone)]
pub struct AdaptiveSchedulingConfig {
    /// Enable adaptive scheduling.
    pub enabled: bool,

    /// Minimum allowed period in nanoseconds.
    pub min_period_ns: u64,

    /// Maximum allowed period in nanoseconds.
    pub max_period_ns: u64,

    /// Step size for period increase when overloaded (nanoseconds).
    pub increase_step_ns: u64,

    /// Step size for period decrease when healthy (nanoseconds).
    pub decrease_step_ns: u64,

    /// Jitter threshold above which period should be relaxed (nanoseconds).
    pub jitter_relax_threshold_ns: u64,

    /// Jitter threshold below which period can tighten (nanoseconds).
    pub jitter_tighten_threshold_ns: u64,

    /// Processing-time threshold above which period should be relaxed (microseconds).
    pub processing_relax_threshold_us: u64,

    /// Processing-time threshold below which period can tighten (microseconds).
    pub processing_tighten_threshold_us: u64,

    /// EMA alpha for processing-time smoothing [0.01, 1.0].
    pub processing_ema_alpha: f64,
}

impl Default for AdaptiveSchedulingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_period_ns: 900_000,              // 0.9ms
            max_period_ns: 1_100_000,            // 1.1ms
            increase_step_ns: 5_000,             // 5us
            decrease_step_ns: 2_000,             // 2us
            jitter_relax_threshold_ns: 200_000,  // 0.2ms
            jitter_tighten_threshold_ns: 50_000, // 0.05ms
            processing_relax_threshold_us: 180,
            processing_tighten_threshold_us: 80,
            processing_ema_alpha: 0.2,
        }
    }
}

impl AdaptiveSchedulingConfig {
    /// Create a new configuration with defaults.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create an enabled configuration with default thresholds.
    pub fn enabled() -> Self {
        Self {
            enabled: true,
            ..Self::default()
        }
    }

    /// Enable or disable adaptive scheduling.
    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    /// Set period bounds.
    pub fn with_period_bounds(mut self, min_ns: u64, max_ns: u64) -> Self {
        self.min_period_ns = min_ns;
        self.max_period_ns = max_ns;
        self
    }

    /// Set step sizes.
    pub fn with_step_sizes(mut self, increase_ns: u64, decrease_ns: u64) -> Self {
        self.increase_step_ns = increase_ns;
        self.decrease_step_ns = decrease_ns;
        self
    }

    /// Set jitter thresholds.
    pub fn with_jitter_thresholds(mut self, relax_ns: u64, tighten_ns: u64) -> Self {
        self.jitter_relax_threshold_ns = relax_ns;
        self.jitter_tighten_threshold_ns = tighten_ns;
        self
    }

    /// Set processing time thresholds.
    pub fn with_processing_thresholds(mut self, relax_us: u64, tighten_us: u64) -> Self {
        self.processing_relax_threshold_us = relax_us;
        self.processing_tighten_threshold_us = tighten_us;
        self
    }

    /// Set EMA alpha.
    pub fn with_ema_alpha(mut self, alpha: f64) -> Self {
        self.processing_ema_alpha = alpha;
        self
    }

    /// Normalize configuration to maintain safe, bounded behavior.
    ///
    /// This ensures:
    /// - min_period_ns <= max_period_ns
    /// - All thresholds are non-zero
    /// - Tighten thresholds <= relax thresholds
    /// - EMA alpha is in [0.01, 1.0]
    pub fn normalize(&mut self) {
        // Swap if min > max
        if self.min_period_ns > self.max_period_ns {
            std::mem::swap(&mut self.min_period_ns, &mut self.max_period_ns);
        }

        // Ensure non-zero minimums
        self.min_period_ns = self.min_period_ns.max(1);
        self.max_period_ns = self.max_period_ns.max(self.min_period_ns);
        self.increase_step_ns = self.increase_step_ns.max(1);
        self.decrease_step_ns = self.decrease_step_ns.max(1);

        // Ensure tighten <= relax
        if self.jitter_tighten_threshold_ns > self.jitter_relax_threshold_ns {
            self.jitter_tighten_threshold_ns = self.jitter_relax_threshold_ns;
        }
        if self.processing_tighten_threshold_us > self.processing_relax_threshold_us {
            self.processing_tighten_threshold_us = self.processing_relax_threshold_us;
        }

        // Clamp EMA alpha
        self.processing_ema_alpha = self.processing_ema_alpha.clamp(0.01, 1.0);
    }

    /// Check if the configuration is valid.
    pub fn is_valid(&self) -> bool {
        self.min_period_ns > 0
            && self.max_period_ns >= self.min_period_ns
            && self.increase_step_ns > 0
            && self.decrease_step_ns > 0
            && self.jitter_tighten_threshold_ns <= self.jitter_relax_threshold_ns
            && self.processing_tighten_threshold_us <= self.processing_relax_threshold_us
            && self.processing_ema_alpha >= 0.01
            && self.processing_ema_alpha <= 1.0
    }
}

/// Snapshot of adaptive scheduling runtime state.
#[derive(Debug, Clone, Copy)]
pub struct AdaptiveSchedulingState {
    /// Whether adaptive scheduling is enabled.
    pub enabled: bool,

    /// Current adaptive target period in nanoseconds.
    pub target_period_ns: u64,

    /// Minimum allowed period in nanoseconds.
    pub min_period_ns: u64,

    /// Maximum allowed period in nanoseconds.
    pub max_period_ns: u64,

    /// Last recorded processing time in microseconds.
    pub last_processing_time_us: u64,

    /// Exponential moving average of processing time in microseconds.
    pub processing_time_ema_us: f64,
}

impl Default for AdaptiveSchedulingState {
    fn default() -> Self {
        Self {
            enabled: false,
            target_period_ns: 1_000_000,
            min_period_ns: 900_000,
            max_period_ns: 1_100_000,
            last_processing_time_us: 0,
            processing_time_ema_us: 0.0,
        }
    }
}

impl AdaptiveSchedulingState {
    /// Create a new state snapshot with default values.
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if the current period is at the maximum.
    pub fn is_at_max(&self) -> bool {
        self.target_period_ns >= self.max_period_ns
    }

    /// Check if the current period is at the minimum.
    pub fn is_at_min(&self) -> bool {
        self.target_period_ns <= self.min_period_ns
    }

    /// Get the period as a fraction of the range [0.0, 1.0].
    pub fn period_fraction(&self) -> f64 {
        if self.max_period_ns == self.min_period_ns {
            return 0.5;
        }
        (self.target_period_ns - self.min_period_ns) as f64
            / (self.max_period_ns - self.min_period_ns) as f64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = AdaptiveSchedulingConfig::default();
        assert!(!config.enabled);
        assert_eq!(config.min_period_ns, 900_000);
        assert_eq!(config.max_period_ns, 1_100_000);
    }

    #[test]
    fn test_enabled_config() {
        let config = AdaptiveSchedulingConfig::enabled();
        assert!(config.enabled);
    }

    #[test]
    fn test_normalize_swaps_min_max() {
        let mut config = AdaptiveSchedulingConfig {
            min_period_ns: 2_000_000,
            max_period_ns: 1_000_000,
            ..Default::default()
        };
        config.normalize();

        assert_eq!(config.min_period_ns, 1_000_000);
        assert_eq!(config.max_period_ns, 2_000_000);
    }

    #[test]
    fn test_normalize_clamps_thresholds() {
        let mut config = AdaptiveSchedulingConfig {
            jitter_tighten_threshold_ns: 300_000,
            jitter_relax_threshold_ns: 200_000,
            processing_tighten_threshold_us: 200,
            processing_relax_threshold_us: 100,
            ..Default::default()
        };
        config.normalize();

        assert_eq!(config.jitter_tighten_threshold_ns, 200_000);
        assert_eq!(config.processing_tighten_threshold_us, 100);
    }

    #[test]
    fn test_normalize_ema_alpha() {
        let mut config = AdaptiveSchedulingConfig {
            processing_ema_alpha: 0.005,
            ..Default::default()
        };
        config.normalize();
        assert!((config.processing_ema_alpha - 0.01).abs() < 1e-10);

        config.processing_ema_alpha = 2.0;
        config.normalize();
        assert!((config.processing_ema_alpha - 1.0).abs() < 1e-10);
    }

    #[test]
    fn test_is_valid() {
        let valid = AdaptiveSchedulingConfig::default();
        assert!(valid.is_valid());

        let invalid = AdaptiveSchedulingConfig {
            min_period_ns: 0,
            ..Default::default()
        };
        assert!(!invalid.is_valid());

        let invalid2 = AdaptiveSchedulingConfig {
            jitter_tighten_threshold_ns: 500_000,
            jitter_relax_threshold_ns: 100_000,
            ..Default::default()
        };
        assert!(!invalid2.is_valid());
    }

    #[test]
    fn test_builder_pattern() {
        let config = AdaptiveSchedulingConfig::new()
            .with_enabled(true)
            .with_period_bounds(800_000, 1_200_000)
            .with_step_sizes(10_000, 5_000)
            .with_jitter_thresholds(300_000, 100_000)
            .with_ema_alpha(0.5);

        assert!(config.enabled);
        assert_eq!(config.min_period_ns, 800_000);
        assert_eq!(config.max_period_ns, 1_200_000);
        assert_eq!(config.increase_step_ns, 10_000);
        assert_eq!(config.decrease_step_ns, 5_000);
        assert!((config.processing_ema_alpha - 0.5).abs() < 1e-10);
    }

    #[test]
    fn test_state_default() {
        let state = AdaptiveSchedulingState::default();
        assert!(!state.enabled);
        assert_eq!(state.target_period_ns, 1_000_000);
    }

    #[test]
    fn test_state_at_bounds() {
        let state = AdaptiveSchedulingState {
            min_period_ns: 900_000,
            max_period_ns: 1_100_000,
            target_period_ns: 900_000,
            ..Default::default()
        };
        assert!(state.is_at_min());
        assert!(!state.is_at_max());

        let state = AdaptiveSchedulingState {
            min_period_ns: 900_000,
            max_period_ns: 1_100_000,
            target_period_ns: 1_100_000,
            ..Default::default()
        };
        assert!(!state.is_at_min());
        assert!(state.is_at_max());
    }

    #[test]
    fn test_state_period_fraction() {
        let state = AdaptiveSchedulingState {
            min_period_ns: 1_000_000,
            max_period_ns: 2_000_000,
            target_period_ns: 1_000_000,
            ..Default::default()
        };
        assert!((state.period_fraction() - 0.0).abs() < 1e-10);

        let state = AdaptiveSchedulingState {
            min_period_ns: 1_000_000,
            max_period_ns: 2_000_000,
            target_period_ns: 1_500_000,
            ..Default::default()
        };
        assert!((state.period_fraction() - 0.5).abs() < 1e-10);

        let state = AdaptiveSchedulingState {
            min_period_ns: 1_000_000,
            max_period_ns: 2_000_000,
            target_period_ns: 2_000_000,
            ..Default::default()
        };
        assert!((state.period_fraction() - 1.0).abs() < 1e-10);
    }
}
