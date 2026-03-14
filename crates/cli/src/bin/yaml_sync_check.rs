//! YAML sync check binary - Verify that two game support matrix YAML files are identical.
//!
//! Usage:
//!     cargo run -p wheelctl --bin yaml-sync-check -- <file_a> <file_b>
//!
//! Exits 0 if the files are structurally identical, 1 if they differ.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]
#![allow(clippy::unwrap_used)]

use serde_yaml::Value;
use std::collections::BTreeSet;
use std::env;
use std::fs;

/// Recursively sort dict keys so comparison is order-independent.
pub(crate) fn sorted_yaml(value: &Value) -> Value {
    match value {
        Value::Mapping(map) => {
            let mut sorted: Vec<(Value, Value)> = map
                .iter()
                .map(|(k, v)| (k.clone(), sorted_yaml(v)))
                .collect();
            sorted.sort_by(|a, b| a.0.as_str().cmp(&b.0.as_str()));
            Value::Mapping(sorted.into_iter().collect())
        }
        Value::Sequence(seq) => Value::Sequence(seq.iter().map(sorted_yaml).collect()),
        _ => value.clone(),
    }
}

/// Return sorted list of "key: name" strings for each game entry.
pub(crate) fn render_games(data: &Value) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(games) = data.get("games").and_then(|v| v.as_mapping()) {
        // Collect keys as strings for sorting
        let mut keys: Vec<String> = games
            .keys()
            .filter_map(|k| k.as_str().map(|s| s.to_string()))
            .collect();
        keys.sort();

        for key in &keys {
            if let Some(game) = games.get(key) {
                let name = game
                    .get("name")
                    .and_then(|n| n.as_str())
                    .unwrap_or(key.as_str());
                lines.push(format!("{key}: {name}"));
            }
        }
    }

    lines
}

fn main() -> std::process::ExitCode {
    let args: Vec<String> = env::args().collect();

    if args.len() != 3 {
        eprintln!("Usage: {} <file_a> <file_b>", args[0]);
        return std::process::ExitCode::from(2);
    }

    let path_a = &args[1];
    let path_b = &args[2];

    // Load and parse YAML files
    let content_a = match fs::read_to_string(path_a) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ERROR: {path_a}: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    let content_b = match fs::read_to_string(path_b) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("ERROR: {path_b}: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    let data_a: Value = match serde_yaml::from_str(&content_a) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ERROR: Failed to parse {path_a}: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    let data_b: Value = match serde_yaml::from_str(&content_b) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("ERROR: Failed to parse {path_b}: {e}");
            return std::process::ExitCode::from(2);
        }
    };

    // Normalize for comparison
    let norm_a = sorted_yaml(&data_a);
    let norm_b = sorted_yaml(&data_b);

    if norm_a == norm_b {
        println!("OK: {path_a} and {path_b} are identical.");
        return std::process::ExitCode::from(0);
    }

    // Build a human-readable diff of the game lists
    let games_a = render_games(&data_a);
    let games_b = render_games(&data_b);

    // Use BTreeSet for efficient set operations
    let set_a: BTreeSet<_> = games_a.iter().cloned().collect();
    let set_b: BTreeSet<_> = games_b.iter().cloned().collect();

    let only_a: Vec<_> = set_a.difference(&set_b).cloned().collect();
    let only_b: Vec<_> = set_b.difference(&set_a).cloned().collect();

    eprintln!("ERROR: game support matrix files are out of sync!");
    eprintln!("  {path_a}");
    eprintln!("  {path_b}");
    eprintln!();

    if !only_a.is_empty() {
        eprintln!("Games only in {path_a}:");
        for g in &only_a {
            eprintln!("  + {g}");
        }
    }

    if !only_b.is_empty() {
        eprintln!("Games only in {path_b}:");
        for g in &only_b {
            eprintln!("  + {g}");
        }
    }

    if only_a.is_empty() && only_b.is_empty() {
        // Same game keys but differing content - show a structured diff
        let text_a = serde_yaml::to_string(&norm_a).unwrap_or_default();
        let text_b = serde_yaml::to_string(&norm_b).unwrap_or_default();

        eprintln!();
        eprintln!("Content diff:");

        let lines_a: Vec<&str> = text_a.lines().collect();
        let lines_b: Vec<&str> = text_b.lines().collect();

        // Simple line-by-line diff
        let max_lines = lines_a.len().max(lines_b.len());
        for i in 0..max_lines {
            let a = lines_a.get(i).copied();
            let b = lines_b.get(i).copied();
            match (a, b) {
                (Some(x), Some(y)) if x == y => {
                    eprintln!("  {x}");
                }
                (Some(x), Some(y)) => {
                    eprintln!("- {x}");
                    eprintln!("+ {y}");
                }
                (Some(x), None) => {
                    eprintln!("- {x}");
                }
                (None, Some(y)) => {
                    eprintln!("+ {y}");
                }
                (None, None) => {}
            }
        }
    }

    eprintln!();
    eprintln!("Fix: update both files to match, or run the single-source-of-truth");
    eprintln!("     generator once it is available (see docs/FRICTION_LOG.md F-001).");

    std::process::ExitCode::from(1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value as YamlValue;

    fn parse_yaml(s: &str) -> YamlValue {
        serde_yaml::from_str(s).unwrap()
    }

    #[test]
    fn test_sorted_yaml_simple_map() {
        let yaml = parse_yaml("b: 2\na: 1");
        let sorted = sorted_yaml(&yaml);
        let text = serde_yaml::to_string(&sorted).unwrap();
        assert!(text.starts_with("a: 1\nb: 2"));
    }

    #[test]
    fn test_sorted_yaml_nested_map() {
        let yaml = parse_yaml("z:\n  b: 2\n  a: 1\na:\n  b: 2");
        let sorted = sorted_yaml(&yaml);
        let text = serde_yaml::to_string(&sorted).unwrap();
        // Top-level keys should be sorted
        assert!(text.starts_with("a:\n"));
        // Nested keys should also be sorted
        assert!(text.contains("a: 1\n  b: 2"));
    }

    #[test]
    fn test_sorted_yaml_array() {
        let yaml = parse_yaml("[3, 1, 2]");
        let sorted = sorted_yaml(&yaml);
        let text = serde_yaml::to_string(&sorted).unwrap();
        // Arrays are not sorted, only maps are sorted
        assert!(text.contains("3") && text.contains("1") && text.contains("2"));
    }

    #[test]
    fn test_sorted_yaml_preserves_values() {
        let yaml = parse_yaml("x: 100\ny: hello");
        let sorted = sorted_yaml(&yaml);
        assert_eq!(sorted.get("x").and_then(|v| v.as_i64()), Some(100));
        assert_eq!(sorted.get("y").and_then(|v| v.as_str()), Some("hello"));
    }

    #[test]
    fn test_render_games_basic() {
        let yaml = parse_yaml(
            r#"
games:
  game_a:
    name: Game A
  game_b:
    name: Game B
"#,
        );
        let games = render_games(&yaml);
        assert_eq!(games.len(), 2);
        assert!(games.contains(&"game_a: Game A".to_string()));
        assert!(games.contains(&"game_b: Game B".to_string()));
    }

    #[test]
    fn test_render_games_sorted() {
        let yaml = parse_yaml(
            r#"
games:
  z_game:
    name: Z Game
  a_game:
    name: A Game
  m_game:
    name: M Game
"#,
        );
        let games = render_games(&yaml);
        assert_eq!(games.len(), 3);
        assert_eq!(games[0], "a_game: A Game");
        assert_eq!(games[1], "m_game: M Game");
        assert_eq!(games[2], "z_game: Z Game");
    }

    #[test]
    fn test_render_games_missing_name() {
        let yaml = parse_yaml(
            r#"
games:
  game_a: {}
"#,
        );
        let games = render_games(&yaml);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0], "game_a: game_a");
    }

    #[test]
    fn test_render_games_no_games() {
        let yaml = parse_yaml("other: data");
        let games = render_games(&yaml);
        assert!(games.is_empty());
    }

    #[test]
    fn test_identical_files() {
        let yaml = parse_yaml("x: 1\ny: 2");
        let norm_a = sorted_yaml(&yaml);
        let norm_b = sorted_yaml(&yaml);
        assert_eq!(norm_a, norm_b);
    }

    #[test]
    fn test_different_files() {
        let yaml_a = parse_yaml("x: 1\ny: 2");
        let yaml_b = parse_yaml("x: 1\ny: 3");
        let norm_a = sorted_yaml(&yaml_a);
        let norm_b = sorted_yaml(&yaml_b);
        assert_ne!(norm_a, norm_b);
    }

    #[test]
    fn test_key_order_independent() {
        // These should be equal after normalization
        let yaml_a = parse_yaml("b: 2\na: 1");
        let yaml_b = parse_yaml("a: 1\nb: 2");
        let norm_a = sorted_yaml(&yaml_a);
        let norm_b = sorted_yaml(&yaml_b);
        assert_eq!(norm_a, norm_b);
    }
}
