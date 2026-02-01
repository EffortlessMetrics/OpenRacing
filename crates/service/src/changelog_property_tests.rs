//! Property-based tests for CHANGELOG format validation.
//!
//! These tests validate that changelog entries can be serialized to markdown
//! and parsed back without loss of information.
//!
//! **Validates: Requirements 1.1, 1.3, 1.5**

use crate::changelog::{ChangelogEntry, ChangelogError};
use chrono::NaiveDate;
use proptest::prelude::*;
use semver::Version;

/// Strategy for generating valid changelog item strings.
///
/// Items must be non-empty, single-line, and not start with special markdown characters.
fn changelog_item_strategy() -> impl Strategy<Value = String> {
    // Generate alphanumeric strings with spaces, avoiding markdown special chars
    "[A-Za-z][A-Za-z0-9 ]{0,50}"
        .prop_filter("Item must not be empty or whitespace-only", |s| {
            !s.trim().is_empty()
        })
        .prop_map(|s| s.trim().to_string())
}

/// Strategy for generating a vector of changelog items.
fn changelog_items_strategy() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(changelog_item_strategy(), 0..5)
}

/// Strategy for generating valid semantic versions.
fn version_strategy() -> impl Strategy<Value = Version> {
    (0u64..100, 0u64..100, 0u64..100)
        .prop_map(|(major, minor, patch)| Version::new(major, minor, patch))
}

/// Strategy for generating valid dates.
///
/// Dates are constrained to a reasonable range for changelog entries.
fn date_strategy() -> impl Strategy<Value = NaiveDate> {
    // Generate dates between 2020-01-01 and 2030-12-31
    (2020i32..=2030, 1u32..=12, 1u32..=28)
        .prop_filter_map("Date must be valid", |(year, month, day)| {
            NaiveDate::from_ymd_opt(year, month, day)
        })
}

/// Strategy for generating valid changelog entries.
fn changelog_entry_strategy() -> impl Strategy<Value = ChangelogEntry> {
    (
        version_strategy(),
        date_strategy(),
        changelog_items_strategy(), // added
        changelog_items_strategy(), // changed
        changelog_items_strategy(), // deprecated
        changelog_items_strategy(), // removed
        changelog_items_strategy(), // fixed
        changelog_items_strategy(), // security
        changelog_items_strategy(), // breaking
    )
        .prop_map(
            |(version, date, added, changed, deprecated, removed, fixed, security, breaking)| {
                ChangelogEntry {
                    version,
                    date,
                    added,
                    changed,
                    deprecated,
                    removed,
                    fixed,
                    security,
                    breaking,
                }
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: release-roadmap-v1, Property 1: CHANGELOG Format Validity
    ///
    /// *For any* valid changelog entry with version, date, and categorized changes,
    /// serializing to markdown and parsing back SHALL produce an equivalent entry structure.
    ///
    /// **Validates: Requirements 1.1, 1.3, 1.5**
    #[test]
    fn prop_changelog_format_roundtrip(entry in changelog_entry_strategy()) {
        // Serialize to markdown
        let markdown = entry.to_markdown();

        // Parse back from markdown
        let parsed_result = ChangelogEntry::from_markdown(&markdown);

        // Parsing should succeed
        prop_assert!(
            parsed_result.is_ok(),
            "Failed to parse markdown: {:?}\nMarkdown:\n{}",
            parsed_result.err(),
            markdown
        );

        let parsed = parsed_result.map_err(|e| TestCaseError::fail(format!("{}", e)))?;

        // Version should match exactly
        prop_assert_eq!(
            entry.version,
            parsed.version,
            "Version mismatch after roundtrip"
        );

        // Date should match exactly
        prop_assert_eq!(
            entry.date,
            parsed.date,
            "Date mismatch after roundtrip"
        );

        // All categorized changes should be preserved
        prop_assert_eq!(
            entry.added,
            parsed.added,
            "Added items mismatch after roundtrip"
        );

        prop_assert_eq!(
            entry.changed,
            parsed.changed,
            "Changed items mismatch after roundtrip"
        );

        prop_assert_eq!(
            entry.deprecated,
            parsed.deprecated,
            "Deprecated items mismatch after roundtrip"
        );

        prop_assert_eq!(
            entry.removed,
            parsed.removed,
            "Removed items mismatch after roundtrip"
        );

        prop_assert_eq!(
            entry.fixed,
            parsed.fixed,
            "Fixed items mismatch after roundtrip"
        );

        prop_assert_eq!(
            entry.security,
            parsed.security,
            "Security items mismatch after roundtrip"
        );

        prop_assert_eq!(
            entry.breaking,
            parsed.breaking,
            "Breaking items mismatch after roundtrip"
        );
    }

    /// Property: Version header parsing is robust
    ///
    /// For any valid version and date, the version header format should be parseable.
    #[test]
    fn prop_version_header_format(
        version in version_strategy(),
        date in date_strategy()
    ) {
        let header = format!("## [{}] - {}", version, date.format("%Y-%m-%d"));
        let entry_markdown = format!("{}\n", header);

        let parsed = ChangelogEntry::from_markdown(&entry_markdown);
        prop_assert!(
            parsed.is_ok(),
            "Failed to parse version header: {}\nError: {:?}",
            header,
            parsed.err()
        );

        let entry = parsed.map_err(|e| TestCaseError::fail(format!("{}", e)))?;
        prop_assert_eq!(entry.version, version);
        prop_assert_eq!(entry.date, date);
    }

    /// Property: ISO 8601 date format is preserved
    ///
    /// Dates should always be serialized in YYYY-MM-DD format per Requirements 1.5.
    #[test]
    fn prop_date_iso8601_format(entry in changelog_entry_strategy()) {
        let markdown = entry.to_markdown();

        // The date should appear in ISO 8601 format (YYYY-MM-DD)
        let expected_date_str = entry.date.format("%Y-%m-%d").to_string();
        prop_assert!(
            markdown.contains(&expected_date_str),
            "Date {} not found in ISO 8601 format in markdown:\n{}",
            expected_date_str,
            markdown
        );
    }

    /// Property: Category sections are correctly formatted
    ///
    /// Each non-empty category should have its own ### section per Requirements 1.3.
    #[test]
    fn prop_category_sections_formatted(entry in changelog_entry_strategy()) {
        let markdown = entry.to_markdown();

        // Check that non-empty categories have their sections
        if !entry.added.is_empty() {
            prop_assert!(
                markdown.contains("### Added"),
                "Missing Added section for non-empty added items"
            );
        }

        if !entry.deprecated.is_empty() {
            prop_assert!(
                markdown.contains("### Deprecated"),
                "Missing Deprecated section for non-empty deprecated items"
            );
        }

        if !entry.removed.is_empty() {
            prop_assert!(
                markdown.contains("### Removed"),
                "Missing Removed section for non-empty removed items"
            );
        }

        if !entry.fixed.is_empty() {
            prop_assert!(
                markdown.contains("### Fixed"),
                "Missing Fixed section for non-empty fixed items"
            );
        }

        if !entry.security.is_empty() {
            prop_assert!(
                markdown.contains("### Security"),
                "Missing Security section for non-empty security items"
            );
        }

        // Changed section appears if there are changed or breaking items
        if !entry.changed.is_empty() || !entry.breaking.is_empty() {
            prop_assert!(
                markdown.contains("### Changed"),
                "Missing Changed section for non-empty changed/breaking items"
            );
        }
    }
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    /// Test that empty entries serialize and parse correctly.
    #[test]
    fn test_empty_entry_roundtrip() -> Result<(), ChangelogError> {
        let entry = ChangelogEntry {
            version: Version::new(1, 0, 0),
            date: NaiveDate::from_ymd_opt(2025, 1, 1)
                .ok_or_else(|| ChangelogError::DateParse("Invalid date".to_string()))?,
            ..Default::default()
        };

        let markdown = entry.to_markdown();
        let parsed = ChangelogEntry::from_markdown(&markdown)?;

        assert_eq!(entry.version, parsed.version);
        assert_eq!(entry.date, parsed.date);
        assert!(parsed.is_empty());

        Ok(())
    }

    /// Test that all categories are preserved in roundtrip.
    #[test]
    fn test_all_categories_roundtrip() -> Result<(), ChangelogError> {
        let entry = ChangelogEntry {
            version: Version::new(2, 1, 0),
            date: NaiveDate::from_ymd_opt(2025, 6, 15)
                .ok_or_else(|| ChangelogError::DateParse("Invalid date".to_string()))?,
            added: vec!["Feature A".to_string()],
            changed: vec!["Behavior B".to_string()],
            deprecated: vec!["Old API C".to_string()],
            removed: vec!["Legacy D".to_string()],
            fixed: vec!["Bug E".to_string()],
            security: vec!["Vulnerability F".to_string()],
            breaking: vec!["API change G".to_string()],
        };

        let markdown = entry.to_markdown();
        let parsed = ChangelogEntry::from_markdown(&markdown)?;

        assert_eq!(entry.version, parsed.version);
        assert_eq!(entry.date, parsed.date);
        assert_eq!(entry.added, parsed.added);
        assert_eq!(entry.changed, parsed.changed);
        assert_eq!(entry.deprecated, parsed.deprecated);
        assert_eq!(entry.removed, parsed.removed);
        assert_eq!(entry.fixed, parsed.fixed);
        assert_eq!(entry.security, parsed.security);
        assert_eq!(entry.breaking, parsed.breaking);

        Ok(())
    }

    /// Test that prerelease versions are handled correctly.
    #[test]
    fn test_prerelease_version() -> Result<(), ChangelogError> {
        let version = Version::parse("1.0.0-alpha.1")
            .map_err(|e| ChangelogError::VersionParse(e.to_string()))?;

        let entry = ChangelogEntry {
            version,
            date: NaiveDate::from_ymd_opt(2025, 1, 1)
                .ok_or_else(|| ChangelogError::DateParse("Invalid date".to_string()))?,
            added: vec!["Alpha feature".to_string()],
            ..Default::default()
        };

        let markdown = entry.to_markdown();
        let parsed = ChangelogEntry::from_markdown(&markdown)?;

        assert_eq!(entry.version, parsed.version);

        Ok(())
    }
}
