//! Hardware version parsing and comparison
//!
//! Provides proper numeric comparison of hardware version strings like "1.2.3".
//! This avoids the lexicographic comparison bug where "10.0" < "2.0" would be false.

use std::cmp::Ordering;
use std::fmt;
use std::str::FromStr;

use thiserror::Error;

/// Error type for hardware version parsing
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum HardwareVersionError {
    /// Version string is empty
    #[error("empty version string")]
    Empty,

    /// Version component is not a valid number
    #[error("invalid version component '{0}': {1}")]
    InvalidComponent(String, String),

    /// Version string contains invalid characters
    #[error("invalid character in version string")]
    InvalidCharacter,
}

/// A hardware version with numeric component comparison
///
/// Parses version strings like "1.2.3" into numeric components and provides
/// proper numeric comparison so that "2.0" < "10.0" works correctly.
///
/// # Examples
///
/// ```ignore
/// use racing_wheel_service::update::hardware_version::HardwareVersion;
///
/// # fn demo() -> Result<(), Box<dyn std::error::Error>> {
/// let v2 = HardwareVersion::parse("2.0")?;
/// let v10 = HardwareVersion::parse("10.0")?;
/// assert!(v2 < v10); // Correct! String comparison would give wrong result.
/// # Ok(())
/// # }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HardwareVersion {
    /// The original version string
    original: String,

    /// Parsed numeric components (e.g., [1, 2, 3] for "1.2.3")
    components: Vec<u32>,
}

impl HardwareVersion {
    /// Parse a hardware version string into a HardwareVersion
    ///
    /// Accepts version strings like "1", "1.2", "1.2.3", etc.
    /// Each component must be a valid unsigned 32-bit integer.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The string is empty
    /// - Any component is not a valid u32
    /// - The string contains invalid characters
    pub fn parse(s: &str) -> Result<Self, HardwareVersionError> {
        let trimmed = s.trim();

        if trimmed.is_empty() {
            return Err(HardwareVersionError::Empty);
        }

        let mut components = Vec::new();

        for part in trimmed.split('.') {
            if part.is_empty() {
                return Err(HardwareVersionError::InvalidComponent(
                    part.to_string(),
                    "empty component".to_string(),
                ));
            }

            let num = part.parse::<u32>().map_err(|e| {
                HardwareVersionError::InvalidComponent(part.to_string(), e.to_string())
            })?;

            components.push(num);
        }

        Ok(Self {
            original: s.to_string(),
            components,
        })
    }

    /// Try to compare two version strings
    ///
    /// Returns `Some(Ordering)` if both strings are valid versions,
    /// or `None` if either string fails to parse.
    ///
    /// This is a convenience method for cases where you want to
    /// fail closed on parse errors.
    pub fn try_compare(a: &str, b: &str) -> Option<Ordering> {
        let va = Self::parse(a).ok()?;
        let vb = Self::parse(b).ok()?;
        Some(va.cmp(&vb))
    }

    /// Get the original version string
    pub fn as_str(&self) -> &str {
        &self.original
    }

    /// Get the numeric components
    pub fn components(&self) -> &[u32] {
        &self.components
    }
}

impl Ord for HardwareVersion {
    fn cmp(&self, other: &Self) -> Ordering {
        // Compare components, treating missing components as 0
        // So "1.2" == "1.2.0" and "1.2" < "1.2.1"
        let max_len = self.components.len().max(other.components.len());

        for i in 0..max_len {
            let a = self.components.get(i).copied().unwrap_or(0);
            let b = other.components.get(i).copied().unwrap_or(0);

            match a.cmp(&b) {
                Ordering::Equal => continue,
                ord => return ord,
            }
        }

        Ordering::Equal
    }
}

impl PartialOrd for HardwareVersion {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl FromStr for HardwareVersion {
    type Err = HardwareVersionError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s)
    }
}

impl fmt::Display for HardwareVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.original)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_version() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1")?;
        assert_eq!(v.components(), &[1]);
        Ok(())
    }

    #[test]
    fn test_parse_two_component_version() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1.2")?;
        assert_eq!(v.components(), &[1, 2]);
        Ok(())
    }

    #[test]
    fn test_parse_three_component_version() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1.2.3")?;
        assert_eq!(v.components(), &[1, 2, 3]);
        Ok(())
    }

    #[test]
    fn test_parse_empty_fails() {
        let result = HardwareVersion::parse("");
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
    }

    #[test]
    fn test_parse_whitespace_only_fails() {
        let result = HardwareVersion::parse("   ");
        assert!(matches!(result, Err(HardwareVersionError::Empty)));
    }

    #[test]
    fn test_parse_invalid_component_fails() {
        let result = HardwareVersion::parse("1.abc.3");
        assert!(matches!(
            result,
            Err(HardwareVersionError::InvalidComponent(..))
        ));
    }

    #[test]
    fn test_parse_negative_component_fails() {
        let result = HardwareVersion::parse("1.-2.3");
        assert!(matches!(
            result,
            Err(HardwareVersionError::InvalidComponent(..))
        ));
    }

    #[test]
    fn test_parse_empty_component_fails() {
        let result = HardwareVersion::parse("1..3");
        assert!(matches!(
            result,
            Err(HardwareVersionError::InvalidComponent(..))
        ));
    }

    #[test]
    fn test_numeric_comparison_10_vs_2() -> Result<(), HardwareVersionError> {
        // This is the critical bug fix test
        // String comparison: "10.0" < "2.0" is TRUE (wrong!)
        // Numeric comparison: 10.0 < 2.0 is FALSE (correct!)
        let v2 = HardwareVersion::parse("2.0")?;
        let v10 = HardwareVersion::parse("10.0")?;

        assert!(v2 < v10, "2.0 should be less than 10.0");
        assert!(v10 > v2, "10.0 should be greater than 2.0");
        Ok(())
    }

    #[test]
    fn test_comparison_1_2_vs_1_2_1() -> Result<(), HardwareVersionError> {
        let v1_2 = HardwareVersion::parse("1.2")?;
        let v1_2_1 = HardwareVersion::parse("1.2.1")?;

        assert!(v1_2 < v1_2_1, "1.2 should be less than 1.2.1");
        Ok(())
    }

    #[test]
    fn test_comparison_1_2_0_equals_1_2() -> Result<(), HardwareVersionError> {
        // Trailing zeros should be treated as equal
        let v1_2 = HardwareVersion::parse("1.2")?;
        let v1_2_0 = HardwareVersion::parse("1.2.0")?;

        assert_eq!(v1_2.cmp(&v1_2_0), Ordering::Equal);
        Ok(())
    }

    #[test]
    fn test_comparison_equal_versions() -> Result<(), HardwareVersionError> {
        let v1 = HardwareVersion::parse("1.2.3")?;
        let v2 = HardwareVersion::parse("1.2.3")?;

        assert_eq!(v1, v2);
        assert_eq!(v1.cmp(&v2), Ordering::Equal);
        Ok(())
    }

    #[test]
    fn test_comparison_chain() -> Result<(), HardwareVersionError> {
        let v1_0 = HardwareVersion::parse("1.0")?;
        let v1_5 = HardwareVersion::parse("1.5")?;
        let v2_0 = HardwareVersion::parse("2.0")?;
        let v10_0 = HardwareVersion::parse("10.0")?;

        assert!(v1_0 < v1_5);
        assert!(v1_5 < v2_0);
        assert!(v2_0 < v10_0);
        Ok(())
    }

    #[test]
    fn test_try_compare_valid() {
        let result = HardwareVersion::try_compare("2.0", "10.0");
        assert_eq!(result, Some(Ordering::Less));
    }

    #[test]
    fn test_try_compare_invalid_returns_none() {
        let result = HardwareVersion::try_compare("invalid", "1.0");
        assert_eq!(result, None);

        let result = HardwareVersion::try_compare("1.0", "invalid");
        assert_eq!(result, None);
    }

    #[test]
    fn test_display() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("1.2.3")?;
        assert_eq!(format!("{}", v), "1.2.3");
        Ok(())
    }

    #[test]
    fn test_from_str() -> Result<(), HardwareVersionError> {
        let v: HardwareVersion = "1.2.3".parse()?;
        assert_eq!(v.components(), &[1, 2, 3]);
        Ok(())
    }

    #[test]
    fn test_large_version_numbers() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("100.200.300")?;
        assert_eq!(v.components(), &[100, 200, 300]);
        Ok(())
    }

    #[test]
    fn test_single_digit_ordering() -> Result<(), HardwareVersionError> {
        let v1 = HardwareVersion::parse("1")?;
        let v2 = HardwareVersion::parse("2")?;
        let v10 = HardwareVersion::parse("10")?;

        assert!(v1 < v2);
        assert!(v2 < v10);
        Ok(())
    }

    #[test]
    fn test_parse_with_whitespace_trim() -> Result<(), HardwareVersionError> {
        let v = HardwareVersion::parse("  1.2.3  ")?;
        assert_eq!(v.components(), &[1, 2, 3]);
        Ok(())
    }
}
