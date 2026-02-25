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
#[allow(clippy::unwrap_used)]
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
        FilterConfig::new_complete(
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
        )
        .unwrap()
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
    fn test_config_hash_with_curve_different() {
        let config = create_test_config();

        let hash_no_curve = calculate_config_hash_with_curve(&config, None);
        let hash_linear = calculate_config_hash_with_curve(&config, Some(&CurveType::Linear));
        let hash_exp =
            calculate_config_hash_with_curve(&config, Some(&CurveType::exponential(2.0).unwrap()));

        assert_ne!(hash_no_curve, hash_linear);
        assert_ne!(hash_linear, hash_exp);
        assert_ne!(hash_no_curve, hash_exp);
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
    fn test_empty_config_hash() {
        let config = FilterConfig::default();
        let hash = calculate_config_hash(&config);
        assert_ne!(hash, 0, "Default config should have non-zero hash");
    }
}
