//! ADR validation binary - Validates that ADR files follow the required format.
//!
//! Usage:
//!     cargo run -p wheelctl --bin validate-adr -- [options]
//!
//! Options:
//!     --adr-dir <path>      Path to ADR directory (default: docs/adr)
//!     --requirements <path> Path to requirements file
//!     -v, --verbose         Verbose output

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![deny(clippy::unwrap_used)]

use clap::Parser;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path to ADR directory
    #[arg(long, default_value = "docs/adr")]
    adr_dir: PathBuf,

    /// Path to requirements file
    #[arg(long, default_value = ".kiro/specs/racing-wheel-suite/requirements.md")]
    requirements: PathBuf,

    /// Verbose output
    #[arg(short, long)]
    verbose: bool,
}

fn find_adr_files(adr_dir: &PathBuf) -> Vec<PathBuf> {
    let mut adr_files = Vec::new();
    let adr_pattern = Regex::new(r"^\d{4}-.*\.md$").unwrap();

    if let Ok(entries) = fs::read_dir(adr_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md")
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name != "template.md"
                && name != "README.md"
                && adr_pattern.is_match(name)
            {
                adr_files.push(path);
            }
        }
    }

    adr_files.sort();
    adr_files
}

fn validate_adr_format(adr_path: &PathBuf) -> Vec<String> {
    let mut errors = Vec::new();

    let content = match fs::read_to_string(adr_path) {
        Ok(c) => c,
        Err(e) => {
            return vec![format!("Could not read file: {e}")];
        }
    };

    let lines: Vec<&str> = content.lines().collect();

    // Check for required sections
    let mut found_sections: HashSet<usize> = HashSet::new();

    let required_patterns = [
        (r"^# ADR-\d{4}: .+", 0),  // Title
        (r"^\*\*Status:\*\*", 1),  // Status
        (r"^\*\*Date:\*\*", 2),    // Date
        (r"^\*\*Authors:\*\*", 3), // Authors
        (r"^## Context", 4),       // Context
        (r"^## Decision", 5),      // Decision
        (r"^## Rationale", 6),     // Rationale
        (r"^## Consequences", 7),  // Consequences
        (r"^## References", 8),    // References
    ];

    // Pre-compile regex patterns
    let compiled_patterns: Vec<(Regex, usize)> = required_patterns
        .iter()
        .filter_map(|(pattern, idx)| Regex::new(pattern).ok().map(|r| (r, *idx)))
        .collect();

    for line in &lines {
        for (regex, idx) in &compiled_patterns {
            if regex.is_match(line) {
                found_sections.insert(*idx);
            }
        }
    }

    let section_names = [
        "Title (# ADR-XXXX: Title)",
        "Status metadata",
        "Date metadata",
        "Authors metadata",
        "Context section",
        "Decision section",
        "Rationale section",
        "Consequences section",
        "References section",
    ];

    let mut missing_sections = Vec::new();
    for (i, name) in section_names.iter().enumerate() {
        if !found_sections.contains(&i) {
            missing_sections.push(*name);
        }
    }

    if !missing_sections.is_empty() {
        errors.push(format!(
            "Missing required sections: {}",
            missing_sections.join(", ")
        ));
    }

    // Check status values
    let status_line = lines.iter().find(|l| l.starts_with("**Status:**"));
    if let Some(status_line) = status_line {
        let valid_statuses = ["Proposed", "Accepted", "Deprecated", "Superseded"];
        let status = status_line.trim().trim_start_matches("**Status:**").trim();
        if !valid_statuses.contains(&status) {
            errors.push(format!(
                "Invalid status '{}'. Must be one of: {}",
                status,
                valid_statuses.join(", ")
            ));
        }
    }

    // Check for requirement references
    if !content.contains("Requirements:") {
        errors.push(
            "No requirement references found. ADRs should reference specific requirement IDs."
                .to_string(),
        );
    }

    errors
}

fn extract_requirement_references(adr_path: &PathBuf) -> HashSet<String> {
    let mut requirements = HashSet::new();
    let req_pattern = Regex::new(r"\b([A-Z]{2,}-\d{2})\b").unwrap();

    if let Ok(content) = fs::read_to_string(adr_path) {
        for cap in req_pattern.captures_iter(&content) {
            if let Some(m) = cap.get(1) {
                requirements.insert(m.as_str().to_string());
            }
        }
    }

    requirements
}

fn validate_requirement_references(
    adr_files: &[PathBuf],
    requirements_file: &PathBuf,
) -> Vec<(String, Vec<String>)> {
    let mut errors = Vec::new();

    if !requirements_file.exists() {
        return vec![(
            "global".to_string(),
            vec!["Requirements file not found".to_string()],
        )];
    }

    let req_content = match fs::read_to_string(requirements_file) {
        Ok(c) => c,
        Err(e) => {
            return vec![(
                "global".to_string(),
                vec![format!("Could not read requirements file: {e}")],
            )];
        }
    };

    let req_pattern = Regex::new(r"\b([A-Z]{2,}-\d{2})\b").unwrap();
    let valid_reqs: HashSet<String> = req_pattern
        .captures_iter(&req_content)
        .filter_map(|cap| cap.get(1).map(|m| m.as_str().to_string()))
        .collect();

    for adr_path in adr_files {
        let mut adr_errors = Vec::new();
        let referenced_reqs = extract_requirement_references(adr_path);

        for req_id in referenced_reqs {
            if !valid_reqs.contains(&req_id) {
                adr_errors.push(format!("References invalid requirement: {}", req_id));
            }
        }

        if !adr_errors.is_empty()
            && let Some(name) = adr_path.file_name().and_then(|n| n.to_str())
        {
            errors.push((name.to_string(), adr_errors));
        }
    }

    errors
}

fn main() -> std::process::ExitCode {
    let args = Args::parse();

    if !args.adr_dir.exists() {
        eprintln!("[ERROR] ADR directory not found: {:?}", args.adr_dir);
        return std::process::ExitCode::from(1);
    }

    println!("[INFO] Validating ADR files...");

    let adr_files = find_adr_files(&args.adr_dir);

    if adr_files.is_empty() {
        eprintln!("[ERROR] No ADR files found");
        return std::process::ExitCode::from(1);
    }

    if args.verbose {
        println!("[INFO] Found {} ADR files", adr_files.len());
    }

    let mut total_errors = 0;

    // Validate format
    for adr_path in &adr_files {
        let errors = validate_adr_format(adr_path);
        if !errors.is_empty() {
            if let Some(name) = adr_path.file_name().and_then(|n| n.to_str()) {
                eprintln!("\n[ERROR] {}:", name);
                for error in &errors {
                    eprintln!("   - {}", error);
                }
            }
            total_errors += errors.len();
        } else if args.verbose
            && let Some(name) = adr_path.file_name().and_then(|n| n.to_str())
        {
            println!("[OK] {}: Format OK", name);
        }
    }

    // Validate requirement references
    let req_errors = validate_requirement_references(&adr_files, &args.requirements);
    for (file_name, errors) in &req_errors {
        if !errors.is_empty() {
            eprintln!("\n[ERROR] {} (requirements):", file_name);
            for error in errors {
                eprintln!("   - {}", error);
            }
            total_errors += errors.len();
        }
    }

    if total_errors == 0 {
        println!("\n[OK] All {} ADR files are valid!", adr_files.len());
        std::process::ExitCode::from(0)
    } else {
        eprintln!("\n[ERROR] Found {} validation errors", total_errors);
        std::process::ExitCode::from(1)
    }
}
