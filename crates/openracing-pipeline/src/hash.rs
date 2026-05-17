//! Pipeline hash calculation for deterministic comparison
//!
//! This module provides deterministic hash calculation for filter configurations,
//! enabling change detection and cache invalidation.

use openracing_curves::CurveLut;
use openracing_curves::CurveType;
use racing_wheel_schemas::entities::FilterConfig;
use racing_wheel_schemas::prelude::CurvePoint;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};

/// Calculate deterministic hash of filter configuration
///
/// This hash is used to detect configuration changes and enable
/// efficient pipeline swap decisions.
///
/// # Arguments
///
/// * `config` - The filter configuration to hash
///
/// # Returns
///
/// A 64-bit hash value that uniquely identifies the configuration
#[must_use]
pub fn calculate_config_hash(config: &FilterConfig) -> u64 {
    let mut hasher = DefaultHasher::new();

    config.reconstruction.hash(&mut hasher);
    config.friction.value().to_bits().hash(&mut hasher);
    config.damper.value().to_bits().hash(&mut hasher);
    config.inertia.value().to_bits().hash(&mut hasher);
    config.slew_rate.value().to_bits().hash(&mut hasher);
    config.torque_cap.value().to_bits().hash(&mut hasher);

    hash_curve_points(&config.curve_points, &mut hasher);
    hash_notch_filters(&config.notch_filters, &mut hasher);
    hash_bumpstop_config(&config.bumpstop, &mut hasher);
    hash_hands_off_config(&config.hands_off, &mut hasher);

    hasher.finish()
}

/// Calculate deterministic hash including response curve
///
/// Extends `calculate_config_hash` to include the response curve type
/// in the hash calculation.
///
/// # Arguments
///
/// * `config` - The filter configuration to hash
/// * `response_curve` - Optional response curve type to include in hash
///
/// # Returns
///
/// A 64-bit hash value that uniquely identifies the configuration with response curve
#[must_use]
pub fn calculate_config_hash_with_curve(
    config: &FilterConfig,
    response_curve: Option<&CurveType>,
) -> u64 {
    let mut hasher = DefaultHasher::new();

    config.reconstruction.hash(&mut hasher);
    config.friction.value().to_bits().hash(&mut hasher);
    config.damper.value().to_bits().hash(&mut hasher);
    config.inertia.value().to_bits().hash(&mut hasher);
    config.slew_rate.value().to_bits().hash(&mut hasher);
    config.torque_cap.value().to_bits().hash(&mut hasher);

    hash_curve_points(&config.curve_points, &mut hasher);
    hash_notch_filters(&config.notch_filters, &mut hasher);
    hash_bumpstop_config(&config.bumpstop, &mut hasher);
    hash_hands_off_config(&config.hands_off, &mut hasher);

    hash_curve_type(response_curve, &mut hasher);

    hasher.finish()
}

/// Hash curve points into the hasher
fn hash_curve_points(curve_points: &[CurvePoint], hasher: &mut DefaultHasher) {
    for point in curve_points {
        point.input.to_bits().hash(hasher);
        point.output.to_bits().hash(hasher);
    }
}

/// Hash notch filters into the hasher
fn hash_notch_filters(
    notch_filters: &[racing_wheel_schemas::entities::NotchFilter],
    hasher: &mut DefaultHasher,
) {
    for filter in notch_filters {
        filter.frequency.value().to_bits().hash(hasher);
        filter.q_factor.to_bits().hash(hasher);
        filter.gain_db.to_bits().hash(hasher);
    }
}

/// Hash bumpstop configuration into the hasher
fn hash_bumpstop_config(
    config: &racing_wheel_schemas::entities::BumpstopConfig,
    hasher: &mut DefaultHasher,
) {
    config.enabled.hash(hasher);
    config.start_angle.to_bits().hash(hasher);
    config.max_angle.to_bits().hash(hasher);
    config.stiffness.to_bits().hash(hasher);
    config.damping.to_bits().hash(hasher);
}

/// Hash hands-off configuration into the hasher
fn hash_hands_off_config(
    config: &racing_wheel_schemas::entities::HandsOffConfig,
    hasher: &mut DefaultHasher,
) {
    config.enabled.hash(hasher);
    config.threshold.to_bits().hash(hasher);
    config.timeout_seconds.to_bits().hash(hasher);
}

/// Hash curve type into the hasher
fn hash_curve_type(curve: Option<&CurveType>, hasher: &mut DefaultHasher) {
    if let Some(curve) = curve {
        match curve {
            CurveType::Linear => {
                0u8.hash(hasher);
            }
            CurveType::Exponential { exponent } => {
                1u8.hash(hasher);
                exponent.to_bits().hash(hasher);
            }
            CurveType::Logarithmic { base } => {
                2u8.hash(hasher);
                base.to_bits().hash(hasher);
            }
            CurveType::Bezier(bezier) => {
                3u8.hash(hasher);
                for (x, y) in &bezier.control_points {
                    x.to_bits().hash(hasher);
                    y.to_bits().hash(hasher);
                }
            }
            CurveType::Custom(lut) => {
                4u8.hash(hasher);
                hash_lut_sample(lut, hasher);
            }
        }
    } else {
        255u8.hash(hasher);
    }
}

/// Hash a sample of LUT values for efficiency
fn hash_lut_sample(lut: &CurveLut, hasher: &mut DefaultHasher) {
    for i in [0, 64, 128, 192, 255] {
        let val = lut.lookup(i as f32 / 255.0);
        val.to_bits().hash(hasher);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use racing_wheel_schemas::prelude::{FrequencyHz, Gain, NotchFilter};

    fn must<T, E: std::fmt::Debug>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(e) => panic!("must() failed: {:?}", e),
        }
    }

    fn create_test_config() -> FilterConfig {
        must(FilterConfig::new_complete(
            4,
            must(Gain::new(0.1)),
            must(Gain::new(0.15)),
            must(Gain::new(0.05)),
            vec![must(NotchFilter::new(
                must(FrequencyHz::new(60.0)),
                2.0,
                -12.0,
            ))],
            must(Gain::new(0.8)),
            vec![
                must(CurvePoint::new(0.0, 0.0)),
                must(CurvePoint::new(0.5, 0.6)),
                must(CurvePoint::new(1.0, 1.0)),
            ],
            must(Gain::new(0.9)),
            racing_wheel_schemas::entities::BumpstopConfig::default(),
            racing_wheel_schemas::entities::HandsOffConfig::default(),
        ))
    }

    fn assert_config_hash_changes(changed_config: FilterConfig, description: &str) {
        let base = create_test_config();
        let base_hash = calculate_config_hash(&base);
        let changed_hash = calculate_config_hash(&changed_config);

        assert_ne!(
            base_hash, changed_hash,
            "changing {description} should change the config hash"
        );

        // Re-hash the changed config to make sure each sensitivity check stays deterministic.
        assert_eq!(changed_hash, calculate_config_hash(&changed_config));
    }

    #[test]
    fn test_config_hash_deterministic() {
        let config = create_test_config();

        let hash1 = calculate_config_hash(&config);
        let hash2 = calculate_config_hash(&config);

        assert_eq!(hash1, hash2, "Same config should produce same hash");
    }

    #[test]
    fn test_config_hash_different_configs() {
        let config1 = create_test_config();
        let config2 = FilterConfig::default();

        let hash1 = calculate_config_hash(&config1);
        let hash2 = calculate_config_hash(&config2);

        assert_ne!(
            hash1, hash2,
            "Different configs should produce different hashes"
        );
    }

    #[test]
    fn test_config_hash_with_curve_different() -> Result<(), openracing_curves::CurveError> {
        let config = create_test_config();

        let hash_no_curve = calculate_config_hash_with_curve(&config, None);
        let hash_linear = calculate_config_hash_with_curve(&config, Some(&CurveType::Linear));
        let exp_curve = CurveType::exponential(2.0)?;
        let hash_exp = calculate_config_hash_with_curve(&config, Some(&exp_curve));

        assert_ne!(hash_no_curve, hash_linear);
        assert_ne!(hash_linear, hash_exp);
        assert_ne!(hash_no_curve, hash_exp);
        Ok(())
    }

    #[test]
    fn test_config_hash_stable_under_ordering() {
        let config = create_test_config();
        let hash1 = calculate_config_hash(&config);
        let hash2 = calculate_config_hash(&config);
        let hash3 = calculate_config_hash(&config);

        assert_eq!(hash1, hash2);
        assert_eq!(hash2, hash3);
    }

    #[test]
    fn test_config_hash_changes_for_scalar_filter_fields() {
        let mut reconstruction = create_test_config();
        reconstruction.reconstruction = 5;
        assert_config_hash_changes(reconstruction, "reconstruction");

        let mut friction = create_test_config();
        friction.friction = must(Gain::new(0.11));
        assert_config_hash_changes(friction, "friction");

        let mut damper = create_test_config();
        damper.damper = must(Gain::new(0.16));
        assert_config_hash_changes(damper, "damper");

        let mut inertia = create_test_config();
        inertia.inertia = must(Gain::new(0.06));
        assert_config_hash_changes(inertia, "inertia");

        let mut slew_rate = create_test_config();
        slew_rate.slew_rate = must(Gain::new(0.7));
        assert_config_hash_changes(slew_rate, "slew rate");

        let mut torque_cap = create_test_config();
        torque_cap.torque_cap = must(Gain::new(0.85));
        assert_config_hash_changes(torque_cap, "torque cap");
    }

    #[test]
    fn test_config_hash_changes_for_curve_and_notch_collections() {
        let mut curve_points = create_test_config();
        curve_points.curve_points[1] = must(CurvePoint::new(0.5, 0.7));
        assert_config_hash_changes(curve_points, "curve points");

        let mut notch_frequency = create_test_config();
        notch_frequency.notch_filters[0].frequency = must(FrequencyHz::new(61.0));
        assert_config_hash_changes(notch_frequency, "notch frequency");

        let mut notch_q = create_test_config();
        notch_q.notch_filters[0].q_factor = 2.5;
        assert_config_hash_changes(notch_q, "notch q factor");

        let mut notch_gain = create_test_config();
        notch_gain.notch_filters[0].gain_db = -10.0;
        assert_config_hash_changes(notch_gain, "notch gain");
    }

    #[test]
    fn test_config_hash_changes_for_safety_envelope_fields() {
        let mut bumpstop_enabled = create_test_config();
        bumpstop_enabled.bumpstop.enabled = false;
        assert_config_hash_changes(bumpstop_enabled, "bumpstop enabled");

        let mut bumpstop_start = create_test_config();
        bumpstop_start.bumpstop.start_angle = 455.0;
        assert_config_hash_changes(bumpstop_start, "bumpstop start angle");

        let mut bumpstop_max = create_test_config();
        bumpstop_max.bumpstop.max_angle = 545.0;
        assert_config_hash_changes(bumpstop_max, "bumpstop max angle");

        let mut bumpstop_stiffness = create_test_config();
        bumpstop_stiffness.bumpstop.stiffness = 0.7;
        assert_config_hash_changes(bumpstop_stiffness, "bumpstop stiffness");

        let mut bumpstop_damping = create_test_config();
        bumpstop_damping.bumpstop.damping = 0.4;
        assert_config_hash_changes(bumpstop_damping, "bumpstop damping");

        let mut hands_off_enabled = create_test_config();
        hands_off_enabled.hands_off.enabled = false;
        assert_config_hash_changes(hands_off_enabled, "hands-off enabled");

        let mut hands_off_threshold = create_test_config();
        hands_off_threshold.hands_off.threshold = 0.07;
        assert_config_hash_changes(hands_off_threshold, "hands-off threshold");

        let mut hands_off_timeout = create_test_config();
        hands_off_timeout.hands_off.timeout_seconds = 4.0;
        assert_config_hash_changes(hands_off_timeout, "hands-off timeout");
    }

    #[test]
    fn test_empty_config_hash() {
        let config = FilterConfig::default();
        let hash = calculate_config_hash(&config);
        assert_ne!(hash, 0, "Default config should have non-zero hash");
    }
}
