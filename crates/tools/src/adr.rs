use std::fs;
use std::path::{Path, PathBuf};

pub fn is_adr_file_name(name: &str) -> bool {
    let Some((number, _rest)) = name.split_once('-') else {
        return false;
    };

    number.len() == 4 && number.chars().all(|c| c.is_ascii_digit()) && name.ends_with(".md")
}

pub fn adr_title(line: &str) -> Option<String> {
    let rest = line.strip_prefix("# ADR-")?;
    let (number, title) = rest.split_once(": ")?;

    (number.len() == 4 && number.chars().all(|c| c.is_ascii_digit()) && !title.is_empty())
        .then(|| format!("ADR-{number}: {title}"))
}

pub fn has_adr_title(line: &str) -> bool {
    adr_title(line).is_some()
}

pub fn find_adr_files(adr_dir: &Path) -> Vec<PathBuf> {
    let mut adr_files = Vec::new();

    if let Ok(entries) = fs::read_dir(adr_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext == "md")
                && let Some(name) = path.file_name().and_then(|n| n.to_str())
                && name != "template.md"
                && name != "README.md"
                && is_adr_file_name(name)
            {
                adr_files.push(path);
            }
        }
    }

    adr_files.sort();
    adr_files
}

pub fn is_iso_date(date: &str) -> bool {
    let mut parts = date.split('-');
    let year = parts.next();
    let month = parts.next();
    let day = parts.next();

    parts.next().is_none()
        && year.is_some_and(|part| part.len() == 4 && part.chars().all(|c| c.is_ascii_digit()))
        && month.is_some_and(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_digit()))
        && day.is_some_and(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_digit()))
}
