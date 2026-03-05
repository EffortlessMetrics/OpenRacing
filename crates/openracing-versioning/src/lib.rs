//! SRP microcrate for protocol version parsing and compatibility checks.
//!
//! This crate intentionally provides pure, allocation-light helpers that are
//! shared by transport/runtime crates.

/// Semantic version represented as three numeric parts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Semver3 {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl Semver3 {
    /// Parse a `major.minor.patch` string.
    pub fn parse(input: &str) -> Option<Self> {
        let mut parts = input.split('.');
        let major = parts.next()?.parse().ok()?;
        let minor = parts.next()?.parse().ok()?;
        let patch = parts.next()?.parse().ok()?;

        // Reject trailing components like 1.2.3.4 for deterministic behavior.
        if parts.next().is_some() {
            return None;
        }

        Some(Self {
            major,
            minor,
            patch,
        })
    }

    /// Return true if this version is compatible with the minimum version.
    ///
    /// Compatibility policy:
    /// - major must match exactly
    /// - minor must be greater than or equal to minimum minor
    /// - when minor matches, patch must be greater than or equal to minimum patch
    pub fn is_compatible_with_min(self, min: Self) -> bool {
        self.major == min.major
            && self.minor >= min.minor
            && (self.minor > min.minor || self.patch >= min.patch)
    }
}

/// Parse and compare semantic versions using OpenRacing compatibility policy.
pub fn is_version_compatible(client_version: &str, min_version: &str) -> bool {
    let Some(client) = Semver3::parse(client_version) else {
        return false;
    };
    let Some(min) = Semver3::parse(min_version) else {
        return false;
    };

    client.is_compatible_with_min(min)
}

#[cfg(test)]
mod tests {
    use super::{Semver3, is_version_compatible};

    #[test]
    fn parse_valid_semver3() -> Result<(), String> {
        let parsed = Semver3::parse("1.2.3").ok_or("expected parse success")?;
        if parsed
            != (Semver3 {
                major: 1,
                minor: 2,
                patch: 3,
            })
        {
            return Err(format!("unexpected parse result: {parsed:?}"));
        }
        Ok(())
    }

    #[test]
    fn parse_invalid_semver3() -> Result<(), String> {
        for input in ["", "1", "1.2", "1.2.x", "1.2.3.4"] {
            if Semver3::parse(input).is_some() {
                return Err(format!("expected parse failure for: {input}"));
            }
        }
        Ok(())
    }

    #[test]
    fn compatibility_policy() -> Result<(), String> {
        let cases = [
            ("1.0.0", "1.0.0", true),
            ("1.1.0", "1.0.0", true),
            ("1.0.1", "1.0.0", true),
            ("0.9.0", "1.0.0", false),
            ("2.0.0", "1.0.0", false),
            ("1.0.0", "1.1.0", false),
            ("invalid", "1.0.0", false),
            ("1.0.0", "invalid", false),
        ];

        for (client, min, expected) in cases {
            let got = is_version_compatible(client, min);
            if got != expected {
                return Err(format!(
                    "compatibility mismatch for client={client}, min={min}: got={got}, expected={expected}"
                ));
            }
        }

        Ok(())
    }
}
