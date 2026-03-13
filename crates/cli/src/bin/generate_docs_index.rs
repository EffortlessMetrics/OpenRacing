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

use clap::Parser;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

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

fn extract_adr_info(adr_path: &PathBuf) -> AdrInfo {
    let mut info = AdrInfo::default();

    let content = match fs::read_to_string(adr_path) {
        Ok(c) => c,
        Err(_) => return info,
    };

    let lines: Vec<&str> = content.lines().collect();

    // Pre-compile regex patterns for metadata extraction
    let title_regex = Regex::new(r"^# (ADR-\d{4}: .+)").unwrap();
    let status_regex = Regex::new(r"^\*\*Status:\*\* (.+)").unwrap();
    let date_regex = Regex::new(r"^\*\*Date:\*\* (.+)").unwrap();
    let authors_regex = Regex::new(r"^\*\*Authors:\*\* (.+)").unwrap();

    // Extract title
    if let Some(first_line) = lines.first() {
        if let Some(cap) = title_regex.captures(first_line) {
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
        if let Some(cap) = status_regex.captures(line) {
            info.status = cap
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
        } else if let Some(cap) = date_regex.captures(line) {
            info.date = cap
                .get(1)
                .map(|m| m.as_str().trim().to_string())
                .unwrap_or_default();
        } else if let Some(cap) = authors_regex.captures(line) {
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

fn generate_adr_index(adr_dir: &PathBuf) -> String {
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
    let date_pattern = Regex::new(r"^\d{4}-\d{2}-\d{2}$").unwrap();

    for adr_path in &adr_files {
        let info = extract_adr_info(adr_path);
        if date_pattern.is_match(&info.date) {
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

fn main() -> std::process::ExitCode {
    let args = Args::parse();

    if !args.adr_dir.exists() {
        eprintln!("[ERROR] ADR directory not found: {:?}", args.adr_dir);
        return std::process::ExitCode::from(1);
    }

    println!("[INFO] Generating documentation index...");

    let index_content = generate_adr_index(&args.adr_dir);

    let index_file = args.adr_dir.join("INDEX.md");

    if let Err(e) = fs::write(&index_file, &index_content) {
        eprintln!("[ERROR] Failed to write index file: {}", e);
        return std::process::ExitCode::from(1);
    }

    println!("[OK] Generated ADR index: {:?}", index_file);
    std::process::ExitCode::from(0)
}
