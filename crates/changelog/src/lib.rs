//! CHANGELOG management following Keep a Changelog format.
//!
//! This crate provides types and utilities for managing CHANGELOG.md files
//! following the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format.

#![deny(static_mut_refs)]

use chrono::NaiveDate;
use semver::Version;
use std::fmt;
use thiserror::Error;

/// Error type for changelog operations.
#[derive(Error, Debug, Clone, PartialEq, Eq)]
pub enum ChangelogError {
    /// Invalid changelog format
    #[error("Invalid changelog format: {0}")]
    InvalidFormat(String),

    /// Failed to parse version
    #[error("Failed to parse version: {0}")]
    VersionParse(String),

    /// Failed to parse date
    #[error("Failed to parse date: {0}")]
    DateParse(String),

    /// Missing required section
    #[error("Missing required section: {0}")]
    MissingSection(String),
}

/// A single changelog entry following Keep a Changelog format.
///
/// Each entry represents a version release with categorized changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChangelogEntry {
    /// Semantic version for this release
    pub version: Version,
    /// Release date in ISO 8601 format
    pub date: NaiveDate,
    /// New features added
    pub added: Vec<String>,
    /// Changes to existing functionality
    pub changed: Vec<String>,
    /// Features marked for removal in future versions
    pub deprecated: Vec<String>,
    /// Features removed in this version
    pub removed: Vec<String>,
    /// Bug fixes
    pub fixed: Vec<String>,
    /// Security-related changes
    pub security: Vec<String>,
    /// Breaking changes (marked with BREAKING prefix)
    pub breaking: Vec<String>,
}

impl Default for ChangelogEntry {
    fn default() -> Self {
        Self {
            version: Version::new(0, 1, 0),
            date: NaiveDate::from_ymd_opt(2025, 1, 1).unwrap_or(NaiveDate::MIN),
            added: Vec::new(),
            changed: Vec::new(),
            deprecated: Vec::new(),
            removed: Vec::new(),
            fixed: Vec::new(),
            security: Vec::new(),
            breaking: Vec::new(),
        }
    }
}

impl ChangelogEntry {
    /// Create a new changelog entry with the given version and date.
    pub fn new(version: Version, date: NaiveDate) -> Self {
        Self {
            version,
            date,
            ..Default::default()
        }
    }

    /// Serialize this entry to Keep a Changelog markdown format.
    pub fn to_markdown(&self) -> String {
        let mut output = String::new();

        // Version header with date
        output.push_str(&format!(
            "## [{}] - {}\n",
            self.version,
            self.date.format("%Y-%m-%d")
        ));

        // Helper to write a section if it has entries
        let write_section = |out: &mut String, title: &str, items: &[String]| {
            if !items.is_empty() {
                out.push_str(&format!("\n### {}\n\n", title));
                for item in items {
                    out.push_str(&format!("- {}\n", item));
                }
            }
        };

        // Write Added section first (standard order)
        write_section(&mut output, "Added", &self.added);

        // Write Changed section with both breaking and regular changes
        if !self.breaking.is_empty() || !self.changed.is_empty() {
            output.push_str("\n### Changed\n\n");
            // Breaking changes first with BREAKING prefix
            for item in &self.breaking {
                output.push_str(&format!("- **BREAKING**: {}\n", item));
            }
            // Then regular changed items
            for item in &self.changed {
                output.push_str(&format!("- {}\n", item));
            }
        }

        write_section(&mut output, "Deprecated", &self.deprecated);
        write_section(&mut output, "Removed", &self.removed);
        write_section(&mut output, "Fixed", &self.fixed);
        write_section(&mut output, "Security", &self.security);

        output
    }

    /// Parse a changelog entry from Keep a Changelog markdown format.
    ///
    /// Expects input starting with a version header line like:
    /// `## [1.0.0] - 2025-01-15`
    pub fn from_markdown(markdown: &str) -> Result<Self, ChangelogError> {
        let lines: Vec<&str> = markdown.lines().collect();

        if lines.is_empty() {
            return Err(ChangelogError::InvalidFormat(
                "Empty changelog entry".to_string(),
            ));
        }

        // Parse version header
        let header = lines[0].trim();
        let (version, date) = parse_version_header(header)?;

        let mut entry = ChangelogEntry::new(version, date);

        // Parse sections
        let mut current_section: Option<&str> = None;

        for line in lines.iter().skip(1) {
            let trimmed = line.trim();

            if trimmed.is_empty() {
                continue;
            }

            // Check for section header
            if let Some(section) = trimmed.strip_prefix("### ") {
                current_section = Some(section.trim());
                continue;
            }

            // Check for list item
            if let Some(item) = trimmed.strip_prefix("- ")
                && let Some(section) = current_section
            {
                // Check for breaking change marker
                if item.starts_with("**BREAKING**:") || item.starts_with("**BREAKING:**") {
                    let breaking_item = item
                        .trim_start_matches("**BREAKING**:")
                        .trim_start_matches("**BREAKING:**")
                        .trim();
                    entry.breaking.push(breaking_item.to_string());
                } else {
                    match section {
                        "Added" => entry.added.push(item.to_string()),
                        "Changed" => entry.changed.push(item.to_string()),
                        "Deprecated" => entry.deprecated.push(item.to_string()),
                        "Removed" => entry.removed.push(item.to_string()),
                        "Fixed" => entry.fixed.push(item.to_string()),
                        "Security" => entry.security.push(item.to_string()),
                        _ => {
                            // Unknown section, ignore
                        }
                    }
                }
            }
        }

        Ok(entry)
    }

    /// Check if this entry has any content.
    pub fn is_empty(&self) -> bool {
        self.added.is_empty()
            && self.changed.is_empty()
            && self.deprecated.is_empty()
            && self.removed.is_empty()
            && self.fixed.is_empty()
            && self.security.is_empty()
            && self.breaking.is_empty()
    }
}

impl fmt::Display for ChangelogEntry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.to_markdown())
    }
}

/// Parse a version header line like `## [1.0.0] - 2025-01-15`
fn parse_version_header(header: &str) -> Result<(Version, NaiveDate), ChangelogError> {
    // Remove leading ## if present
    let header = header.trim_start_matches('#').trim();

    // Find version in brackets
    let start = header
        .find('[')
        .ok_or_else(|| ChangelogError::InvalidFormat("Missing version brackets".to_string()))?;
    let end = header
        .find(']')
        .ok_or_else(|| ChangelogError::InvalidFormat("Missing version brackets".to_string()))?;

    if start >= end {
        return Err(ChangelogError::InvalidFormat(
            "Invalid version brackets".to_string(),
        ));
    }

    let version_str = &header[start + 1..end];
    let version = Version::parse(version_str)
        .map_err(|e| ChangelogError::VersionParse(format!("{}: {}", version_str, e)))?;

    // Find date after the dash
    let date_part = header[end + 1..].trim().trim_start_matches('-').trim();

    let date = NaiveDate::parse_from_str(date_part, "%Y-%m-%d")
        .map_err(|e| ChangelogError::DateParse(format!("{}: {}", date_part, e)))?;

    Ok((version, date))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_changelog_entry_roundtrip() -> Result<(), ChangelogError> {
        let entry = ChangelogEntry {
            version: Version::new(1, 0, 0),
            date: NaiveDate::from_ymd_opt(2025, 6, 15)
                .ok_or_else(|| ChangelogError::DateParse("Invalid date".to_string()))?,
            added: vec!["New feature A".to_string(), "New feature B".to_string()],
            changed: vec!["Updated behavior X".to_string()],
            deprecated: vec![],
            removed: vec!["Old feature Y".to_string()],
            fixed: vec!["Bug fix Z".to_string()],
            security: vec![],
            breaking: vec![],
        };

        let markdown = entry.to_markdown();
        let parsed = ChangelogEntry::from_markdown(&markdown)?;

        assert_eq!(entry.version, parsed.version);
        assert_eq!(entry.date, parsed.date);
        assert_eq!(entry.added, parsed.added);
        assert_eq!(entry.changed, parsed.changed);
        assert_eq!(entry.removed, parsed.removed);
        assert_eq!(entry.fixed, parsed.fixed);

        Ok(())
    }

    #[test]
    fn test_parse_version_header() -> Result<(), ChangelogError> {
        let (version, date) = parse_version_header("## [1.2.3] - 2025-06-15")?;
        assert_eq!(version, Version::new(1, 2, 3));
        assert_eq!(
            date,
            NaiveDate::from_ymd_opt(2025, 6, 15)
                .ok_or_else(|| ChangelogError::DateParse("Invalid date".to_string()))?
        );
        Ok(())
    }

    #[test]
    fn test_breaking_change_serialization() -> Result<(), ChangelogError> {
        let entry = ChangelogEntry {
            version: Version::new(2, 0, 0),
            date: NaiveDate::from_ymd_opt(2025, 1, 1)
                .ok_or_else(|| ChangelogError::DateParse("Invalid date".to_string()))?,
            breaking: vec!["API changed incompatibly".to_string()],
            ..Default::default()
        };

        let markdown = entry.to_markdown();
        assert!(markdown.contains("**BREAKING**"));

        Ok(())
    }
}
