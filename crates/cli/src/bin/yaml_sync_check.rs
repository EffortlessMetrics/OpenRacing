//! YAML sync tool - verify or repair game support matrix YAML mirrors.
//!
//! Usage:
//!     cargo run -p wheelctl --bin yaml-sync-check -- --check
//!     cargo run -p wheelctl --bin yaml-sync-check -- --fix
//!     cargo run -p wheelctl --bin yaml-sync-check -- <file_a> <file_b>
//!
//! Exits 0 if the files are structurally identical, 1 if they differ in
//! check mode, and 2 for usage / I/O / parse errors.

#![deny(static_mut_refs)]
#![deny(unused_must_use)]

use clap::Parser;
use serde_yaml::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

const CANONICAL: &str = "crates/telemetry-config/src/game_support_matrix.yaml";
const MIRROR: &str = "crates/telemetry-support/src/game_support_matrix.yaml";

#[derive(Debug, Parser)]
#[command(
    author,
    version,
    about = "Check or fix OpenRacing game support matrix YAML sync"
)]
struct Args {
    /// Check that the files are in sync without modifying anything.
    #[arg(long, conflicts_with = "fix")]
    check: bool,

    /// Copy the canonical file to the mirror when they differ.
    #[arg(long, conflicts_with = "check")]
    fix: bool,

    /// Optional file pair to compare. Defaults to the canonical telemetry
    /// config matrix and telemetry support mirror.
    #[arg(value_name = "FILE", num_args = 0..=2)]
    files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    Check,
    Fix,
}

#[derive(Debug)]
struct Config {
    mode: Mode,
    source: PathBuf,
    mirror: PathBuf,
}

impl Config {
    fn from_args(args: Args) -> Result<Self, String> {
        let mode = if args.fix { Mode::Fix } else { Mode::Check };
        let (source, mirror) = match args.files.as_slice() {
            [] => (PathBuf::from(CANONICAL), PathBuf::from(MIRROR)),
            [source, mirror] => (source.clone(), mirror.clone()),
            [_] => return Err("expected either zero paths or both <file_a> <file_b>".to_string()),
            _ => return Err("expected at most two paths: <file_a> <file_b>".to_string()),
        };

        Ok(Self {
            mode,
            source,
            mirror,
        })
    }
}

/// Recursively sort dict keys so comparison is order-independent.
pub(crate) fn sorted_yaml(value: &Value) -> Value {
    match value {
        Value::Mapping(map) => {
            let mut sorted: Vec<(Value, Value)> = map
                .iter()
                .map(|(key, value)| (key.clone(), sorted_yaml(value)))
                .collect();
            sorted.sort_by(|left, right| left.0.as_str().cmp(&right.0.as_str()));
            Value::Mapping(sorted.into_iter().collect())
        }
        Value::Sequence(seq) => Value::Sequence(seq.iter().map(sorted_yaml).collect()),
        _ => value.clone(),
    }
}

/// Return sorted list of "key: name" strings for each game entry.
pub(crate) fn render_games(data: &Value) -> Vec<String> {
    let mut lines = Vec::new();

    if let Some(games) = data.get("games").and_then(|value| value.as_mapping()) {
        let mut keys: Vec<String> = games
            .keys()
            .filter_map(|key| key.as_str().map(str::to_string))
            .collect();
        keys.sort();

        for key in &keys {
            if let Some(game) = games.get(key) {
                let name = game
                    .get("name")
                    .and_then(|name| name.as_str())
                    .unwrap_or(key.as_str());
                lines.push(format!("{key}: {name}"));
            }
        }
    }

    lines
}

fn main() -> std::process::ExitCode {
    let args = Args::parse();
    let config = match Config::from_args(args) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("ERROR: {message}");
            eprintln!("Usage: yaml-sync-check [--check | --fix] [<file_a> <file_b>]");
            return std::process::ExitCode::from(2);
        }
    };

    match run(&config) {
        Ok(()) => std::process::ExitCode::from(0),
        Err(AppError::Different) => std::process::ExitCode::from(1),
        Err(AppError::Fatal(message)) => {
            eprintln!("ERROR: {message}");
            std::process::ExitCode::from(2)
        }
    }
}

#[derive(Debug)]
enum AppError {
    Different,
    Fatal(String),
}

fn run(config: &Config) -> Result<(), AppError> {
    let content_a = read_text(&config.source)?;
    let content_b = read_text(&config.mirror)?;
    let data_a = parse_yaml(&config.source, &content_a)?;
    let data_b = parse_yaml(&config.mirror, &content_b)?;

    let norm_a = sorted_yaml(&data_a);
    let norm_b = sorted_yaml(&data_b);

    if norm_a == norm_b {
        println!(
            "OK: {} and {} are identical.",
            config.source.display(),
            config.mirror.display()
        );
        return Ok(());
    }

    report_difference(config, &data_a, &data_b, &norm_a, &norm_b);

    match config.mode {
        Mode::Check => Err(AppError::Different),
        Mode::Fix => {
            fs::copy(&config.source, &config.mirror).map_err(|err| {
                AppError::Fatal(format!(
                    "failed to copy {} to {}: {err}",
                    config.source.display(),
                    config.mirror.display()
                ))
            })?;
            println!(
                "Fixed: copied {} -> {}",
                config.source.display(),
                config.mirror.display()
            );
            Ok(())
        }
    }
}

fn read_text(path: &Path) -> Result<String, AppError> {
    fs::read_to_string(path).map_err(|err| AppError::Fatal(format!("{}: {err}", path.display())))
}

fn parse_yaml(path: &Path, content: &str) -> Result<Value, AppError> {
    serde_yaml::from_str(content)
        .map_err(|err| AppError::Fatal(format!("failed to parse {}: {err}", path.display())))
}

fn report_difference(
    config: &Config,
    data_a: &Value,
    data_b: &Value,
    norm_a: &Value,
    norm_b: &Value,
) {
    let games_a = render_games(data_a);
    let games_b = render_games(data_b);

    let set_a: BTreeSet<_> = games_a.iter().cloned().collect();
    let set_b: BTreeSet<_> = games_b.iter().cloned().collect();

    let only_a: Vec<_> = set_a.difference(&set_b).cloned().collect();
    let only_b: Vec<_> = set_b.difference(&set_a).cloned().collect();

    eprintln!("ERROR: game support matrix files are out of sync!");
    eprintln!("  canonical : {}", config.source.display());
    eprintln!("  mirror    : {}", config.mirror.display());
    eprintln!();

    if !only_a.is_empty() {
        eprintln!("Games only in {}:", config.source.display());
        for game in &only_a {
            eprintln!("  + {game}");
        }
    }

    if !only_b.is_empty() {
        eprintln!("Games only in {}:", config.mirror.display());
        for game in &only_b {
            eprintln!("  + {game}");
        }
    }

    if only_a.is_empty() && only_b.is_empty() {
        eprintln!();
        eprintln!("Content diff:");
        show_line_diff(norm_a, norm_b);
    }

    eprintln!();
    match config.mode {
        Mode::Check => {
            eprintln!(
                "Run `cargo run -p wheelctl --bin yaml-sync-check -- --fix` to copy the canonical version to the mirror."
            );
        }
        Mode::Fix => {
            eprintln!("Repairing by copying canonical -> mirror.");
        }
    }
}

fn show_line_diff(norm_a: &Value, norm_b: &Value) {
    let text_a = serde_yaml::to_string(norm_a)
        .unwrap_or_else(|err| format!("<failed to render left YAML: {err}>"));
    let text_b = serde_yaml::to_string(norm_b)
        .unwrap_or_else(|err| format!("<failed to render right YAML: {err}>"));

    let lines_a: Vec<&str> = text_a.lines().collect();
    let lines_b: Vec<&str> = text_b.lines().collect();
    let max_lines = lines_a.len().max(lines_b.len());

    for index in 0..max_lines {
        let a = lines_a.get(index).copied();
        let b = lines_b.get(index).copied();
        match (a, b) {
            (Some(left), Some(right)) if left == right => eprintln!("  {left}"),
            (Some(left), Some(right)) => {
                eprintln!("- {left}");
                eprintln!("+ {right}");
            }
            (Some(left), None) => eprintln!("- {left}"),
            (None, Some(right)) => eprintln!("+ {right}"),
            (None, None) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_yaml::Value as YamlValue;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn parse_yaml_value(s: &str) -> Result<YamlValue, serde_yaml::Error> {
        serde_yaml::from_str(s)
    }

    #[test]
    fn test_sorted_yaml_simple_map() -> TestResult {
        let yaml = parse_yaml_value("b: 2\na: 1")?;
        let sorted = sorted_yaml(&yaml);
        let text = serde_yaml::to_string(&sorted)?;
        assert!(text.starts_with("a: 1\nb: 2"));
        Ok(())
    }

    #[test]
    fn test_sorted_yaml_nested_map() -> TestResult {
        let yaml = parse_yaml_value("z:\n  b: 2\n  a: 1\na:\n  b: 2")?;
        let sorted = sorted_yaml(&yaml);
        let text = serde_yaml::to_string(&sorted)?;
        assert!(text.starts_with("a:\n"));
        assert!(text.contains("a: 1\n  b: 2"));
        Ok(())
    }

    #[test]
    fn test_sorted_yaml_array() -> TestResult {
        let yaml = parse_yaml_value("[3, 1, 2]")?;
        let sorted = sorted_yaml(&yaml);
        let text = serde_yaml::to_string(&sorted)?;
        assert!(text.contains("3") && text.contains("1") && text.contains("2"));
        Ok(())
    }

    #[test]
    fn test_sorted_yaml_preserves_values() -> TestResult {
        let yaml = parse_yaml_value("x: 100\ny: hello")?;
        let sorted = sorted_yaml(&yaml);
        assert_eq!(sorted.get("x").and_then(|value| value.as_i64()), Some(100));
        assert_eq!(
            sorted.get("y").and_then(|value| value.as_str()),
            Some("hello")
        );
        Ok(())
    }

    #[test]
    fn test_render_games_basic() -> TestResult {
        let yaml = parse_yaml_value(
            r#"
games:
  game_a:
    name: Game A
  game_b:
    name: Game B
"#,
        )?;
        let games = render_games(&yaml);
        assert_eq!(games.len(), 2);
        assert!(games.contains(&"game_a: Game A".to_string()));
        assert!(games.contains(&"game_b: Game B".to_string()));
        Ok(())
    }

    #[test]
    fn test_render_games_sorted() -> TestResult {
        let yaml = parse_yaml_value(
            r#"
games:
  z_game:
    name: Z Game
  a_game:
    name: A Game
  m_game:
    name: M Game
"#,
        )?;
        let games = render_games(&yaml);
        assert_eq!(games.len(), 3);
        assert_eq!(games[0], "a_game: A Game");
        assert_eq!(games[1], "m_game: M Game");
        assert_eq!(games[2], "z_game: Z Game");
        Ok(())
    }

    #[test]
    fn test_render_games_missing_name() -> TestResult {
        let yaml = parse_yaml_value(
            r#"
games:
  game_a: {}
"#,
        )?;
        let games = render_games(&yaml);
        assert_eq!(games.len(), 1);
        assert_eq!(games[0], "game_a: game_a");
        Ok(())
    }

    #[test]
    fn test_render_games_no_games() -> TestResult {
        let yaml = parse_yaml_value("other: data")?;
        let games = render_games(&yaml);
        assert!(games.is_empty());
        Ok(())
    }

    #[test]
    fn test_identical_files() -> TestResult {
        let yaml = parse_yaml_value("x: 1\ny: 2")?;
        let norm_a = sorted_yaml(&yaml);
        let norm_b = sorted_yaml(&yaml);
        assert_eq!(norm_a, norm_b);
        Ok(())
    }

    #[test]
    fn test_different_files() -> TestResult {
        let yaml_a = parse_yaml_value("x: 1\ny: 2")?;
        let yaml_b = parse_yaml_value("x: 1\ny: 3")?;
        let norm_a = sorted_yaml(&yaml_a);
        let norm_b = sorted_yaml(&yaml_b);
        assert_ne!(norm_a, norm_b);
        Ok(())
    }

    #[test]
    fn test_key_order_independent() -> TestResult {
        let yaml_a = parse_yaml_value("b: 2\na: 1")?;
        let yaml_b = parse_yaml_value("a: 1\nb: 2")?;
        let norm_a = sorted_yaml(&yaml_a);
        let norm_b = sorted_yaml(&yaml_b);
        assert_eq!(norm_a, norm_b);
        Ok(())
    }

    #[test]
    fn default_config_uses_check_mode_and_repository_paths() -> TestResult {
        let args = Args::try_parse_from(["yaml-sync-check"])?;
        let config = Config::from_args(args)?;
        assert_eq!(config.mode, Mode::Check);
        assert_eq!(config.source, PathBuf::from(CANONICAL));
        assert_eq!(config.mirror, PathBuf::from(MIRROR));
        Ok(())
    }

    #[test]
    fn fix_config_accepts_custom_pair() -> TestResult {
        let args = Args::try_parse_from(["yaml-sync-check", "--fix", "a.yaml", "b.yaml"])?;
        let config = Config::from_args(args)?;
        assert_eq!(config.mode, Mode::Fix);
        assert_eq!(config.source, PathBuf::from("a.yaml"));
        assert_eq!(config.mirror, PathBuf::from("b.yaml"));
        Ok(())
    }

    #[test]
    fn config_rejects_single_path() -> TestResult {
        let args = Args::try_parse_from(["yaml-sync-check", "only.yaml"])?;
        let result = Config::from_args(args);
        assert!(result.is_err(), "single path should be rejected");
        if let Err(err) = result {
            assert!(err.contains("zero paths or both"));
        }
        Ok(())
    }
}
