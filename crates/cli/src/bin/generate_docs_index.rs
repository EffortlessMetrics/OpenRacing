//! Documentation index generator - Generates an index of all ADRs.
//!
//! Usage:
//!     cargo run -p wheelctl --bin generate-docs-index -- [options]
//!
//! Options:
//!     --adr-dir <path>  Path to ADR directory (default: docs/adr)

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

use anyhow::{Context, Result};
use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static TITLE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^# (ADR-\d{4}: .+)").expect("invalid title regex"));
static STATUS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\*\*Status:\*\* (.+)").expect("invalid status regex"));
static DATE_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\*\*Date:\*\* (.+)").expect("invalid date regex"));
static AUTHORS_REGEX: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\*\*Authors:\*\* (.+)").expect("invalid authors regex"));
static ADR_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-.*\.md$").expect("invalid adr pattern"));
static DATE_PATTERN: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}$").expect("invalid date pattern"));

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to ADR directory
    #[arg(long, default_value = "docs/adr")]
    adr_dir: PathBuf,
}

#[derive(Debug, Default)]
struct AdrInfo {
    title: String,
    description: String,
    status: String,
    date: String,
    authors: String,
}

pub(crate) fn extract_adr_info(adr_path: &PathBuf) -> AdrInfo {
    let mut info = AdrInfo::default();

    let content = match fs::read_to_string(adr_path) {
        Ok(c) => c,
        Err(_) => return info,
    };

    let lines: Vec<&str> = content.lines().collect();

    // Extract title
    if let Some(first_line) = lines.first() {
        if let Some(cap) = TITLE_REGEX.captures(first_line) {
            info.title = cap
                .get(1)
                .map(|m| m.as_str().to_string())
                .unwrap_or_default();
        }
        if info.title.is_empty() {
            info.title = adr_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("")
                .to_string();
        }
    }

    // Extract metadata
    for line in lines.iter().take(20) {
        if let Some(cap) = STATUS_REGEX.captures(line) {
            info.status = cap
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
        } else if let Some(cap) = DATE_REGEX.captures(line) {
            info.date = cap
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
        } else if let Some(cap) = AUTHORS_REGEX.captures(line) {
            info.authors = cap
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
        }
    }

    // Extract first paragraph of context as description
    let mut context_started = false;
    for line in &lines {
        if line.starts_with("## Context") {
            context_started = true;
            continue;
        }
        if context_started {
            if line.starts_with("##") {
                break;
            }
            if !line.trim().is_empty() && info.description.is_empty() {
                info.description = line.trim().to_string();
                break;
            }
        }
    }

    if info.status.is_empty() {
        info.status = "Unknown".to_string();
    }
    if info.date.is_empty() {
        info.date = "Unknown".to_string();
    }
    if info.authors.is_empty() {
        info.authors = "Unknown".to_string();
    }

    info
}

pub(crate) fn generate_adr_index(adr_dir: &Path) -> String {
    let mut adr_files = Vec::new();

    if let Ok(entries) = fs::read_dir(adr_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md")
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name != "template.md"
                && name != "README.md"
                && ADR_PATTERN.is_match(name)
            {
                adr_files.push(path);
            }
        }
    }

    adr_files.sort();

    let mut index_lines = Vec::new();

    index_lines.push("# Architecture Decision Records Index".to_string());
    index_lines.push(String::new());
    index_lines.push(format!("Total ADRs: {}", adr_files.len()));
    index_lines.push(String::new());
    index_lines.push("| ADR | Title | Status | Date | Authors |".to_string());
    index_lines.push("|-----|-------|--------|------|---------|".to_string());

    for adr_path in &adr_files {
        let info = extract_adr_info(adr_path);
        let file_name = adr_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        let adr_num = file_name.chars().take(4).collect::<String>();

        index_lines.push(format!(
            "| [{}]({}) | {} | {} | {} | {} |",
            adr_num, file_name, info.title, info.status, info.date, info.authors
        ));
    }

    index_lines.push(String::new());
    index_lines.push("## Status Summary".to_string());
    index_lines.push(String::new());

    // Count by status
    let mut status_counts: HashMap<String, usize> = HashMap::new();
    for adr_path in &adr_files {
        let info = extract_adr_info(adr_path);
        *status_counts.entry(info.status).or_insert(0) += 1;
    }

    let mut statuses: Vec<_> = status_counts.keys().collect();
    statuses.sort();
    for status in statuses {
        if let Some(count) = status_counts.get(status) {
            index_lines.push(format!("- **{}**: {}", status, count));
        }
    }

    index_lines.push(String::new());
    index_lines.push("## Recent Changes".to_string());
    index_lines.push(String::new());

    // Sort by date (newest first)
    let mut dated_adrs: Vec<(String, &PathBuf, AdrInfo)> = Vec::new();

    for adr_path in &adr_files {
        let info = extract_adr_info(adr_path);
        if DATE_PATTERN.is_match(&info.date) {
            dated_adrs.push((info.date.clone(), adr_path, info));
        }
    }

    dated_adrs.sort_by(|a, b| b.0.cmp(&a.0));

    for (_, adr_path, info) in dated_adrs.iter().take(5) {
        let file_name = adr_path.file_name().and_then(|n| n.to_str()).unwrap_or("");

        index_lines.push(format!("- {}: [{}]({})", info.date, info.title, file_name));
    }

    index_lines.join("\n")
}

fn main() -> Result<()> {
    let args = Args::parse();

    if !args.adr_dir.exists() {
        anyhow::bail!("ADR directory not found: {:?}", args.adr_dir);
    }

    println!("[INFO] Generating documentation index...");

    let index_content = generate_adr_index(&args.adr_dir);

    let index_file = args.adr_dir.join("INDEX.md");

    fs::write(&index_file, &index_content)
        .with_context(|| format!("Failed to write index to {:?}", index_file))?;

    println!("[OK] Generated ADR index: {:?}", index_file);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    fn create_temp_adr(name: &str, content: &str) -> Result<(TempDir, PathBuf)> {
        let temp_dir = TempDir::new().context("tempdir_new")?;
        let file_path = temp_dir.path().join(name);
        let mut file = std::fs::File::create(&file_path).context("file_create")?;
        file.write_all(content.as_bytes()).context("write_all")?;
        Ok((temp_dir, file_path))
    }

    #[test]
    fn test_extract_adr_info_basic() -> Result<()> {
        let (_temp_dir, adr_path) = create_temp_adr(
            "0001-test.md",
            r#"# ADR-0001: Test Title

**Status:** Proposed
**Date:** 2026-01-15
**Authors:** Test Author

## Context
This is a test context.

## Decision
This is a test decision.

## Rationale
Test rationale.

## Consequences
Test consequences.
"#,
        )?;
        let info = extract_adr_info(&adr_path);
        assert_eq!(info.title, "ADR-0001: Test Title");
        assert_eq!(info.status, "Proposed");
        assert_eq!(info.date, "2026-01-15");
        assert_eq!(info.authors, "Test Author");
        Ok(())
    }

    #[test]
    fn test_extract_adr_info_extracts_description() -> Result<()> {
        let (_temp_dir, adr_path) = create_temp_adr(
            "0001-test.md",
            r#"# ADR-0001: Test Title

**Status:** Proposed
**Date:** 2026-01-15
**Authors:** Test Author

## Context
This is the first line of context.
And this is the second line.

## Decision
This is a test decision.
"#,
        )?;
        let info = extract_adr_info(&adr_path);
        assert_eq!(info.description, "This is the first line of context.");
        Ok(())
    }

    #[test]
    fn test_extract_adr_info_defaults() -> Result<()> {
        // Test with minimal ADR content - title regex won't match because
        // there's no colon and description after ADR number, so it falls back to filename
        let (_temp_dir, adr_path) =
            create_temp_adr("0001-test.md", "# ADR-0001\nNo metadata here.")?;
        let info = extract_adr_info(&adr_path);
        // Falls back to filename stem when title regex doesn't match
        assert_eq!(info.title, "0001-test");
        assert_eq!(info.status, "Unknown");
        assert_eq!(info.date, "Unknown");
        assert_eq!(info.authors, "Unknown");
        Ok(())
    }

    #[test]
    fn test_generate_adr_index() -> Result<()> {
        let temp_dir = TempDir::new().context("tempdir_new")?;

        // Create multiple ADR files
        let adr1 = temp_dir.path().join("0001-first.md");
        std::fs::write(
            &adr1,
            r#"# ADR-0001: First ADR

**Status:** Proposed
**Date:** 2026-01-15
**Authors:** Author One

## Context
Context 1.

## Decision
Decision 1.

## Rationale
Rationale 1.

## Consequences
Consequences 1.
"#,
        )
        .context("write_adr1")?;

        let adr2 = temp_dir.path().join("0002-second.md");
        std::fs::write(
            &adr2,
            r#"# ADR-0002: Second ADR

**Status:** Accepted
**Date:** 2026-01-10
**Authors:** Author Two

## Context
Context 2.

## Decision
Decision 2.

## Rationale
Rationale 2.

## Consequences
Consequences 2.
"#,
        )
        .context("write_adr2")?;

        let index = generate_adr_index(temp_dir.path());

        // Check index content
        assert!(index.contains("Architecture Decision Records Index"));
        assert!(index.contains("Total ADRs: 2"));
        assert!(index.contains("ADR-0001"));
        assert!(index.contains("ADR-0002"));
        assert!(index.contains("Proposed"));
        assert!(index.contains("Accepted"));
        assert!(index.contains("Status Summary"));
        Ok(())
    }

    #[test]
    fn test_generate_adr_index_status_summary() -> Result<()> {
        let temp_dir = TempDir::new().context("tempdir_new")?;

        let adr1 = temp_dir.path().join("0001-proposed.md");
        std::fs::write(
            &adr1,
            r#"# ADR-0001: Proposed ADR

**Status:** Proposed
**Date:** 2026-01-15
**Authors:** Author

## Context
Context.

## Decision
Decision.

## Rationale
Rationale.

## Consequences
Consequences.
"#,
        )
        .context("write_adr1")?;

        let adr2 = temp_dir.path().join("0002-accepted.md");
        std::fs::write(
            &adr2,
            r#"# ADR-0002: Accepted ADR

**Status:** Accepted
**Date:** 2026-01-10
**Authors:** Author

## Context
Context.

## Decision
Decision.

## Rationale
Rationale.

## Consequences
Consequences.
"#,
        )
        .context("write_adr2")?;

        let index = generate_adr_index(temp_dir.path());

        assert!(index.contains("**Proposed**: 1"));
        assert!(index.contains("**Accepted**: 1"));
        Ok(())
    }

    #[test]
    fn test_adr_info_default() {
        let info = AdrInfo::default();
        assert!(info.title.is_empty());
        assert!(info.description.is_empty());
        assert!(info.status.is_empty());
        assert!(info.date.is_empty());
        assert!(info.authors.is_empty());
    }
}
