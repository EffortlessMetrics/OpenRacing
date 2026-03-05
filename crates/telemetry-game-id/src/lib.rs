//! Canonical telemetry game-id normalization.
//!
//! This microcrate intentionally owns only historical alias normalization so that
//! multiple telemetry crates can share one deterministic mapping.

#![deny(static_mut_refs)]

/// Normalize telemetry game IDs at crate boundaries.
///
/// This function is allocation-free and returns either the original `game_id`
/// slice or a canonical static alias.
pub fn normalize_game_id(game_id: &str) -> &str {
    if game_id.eq_ignore_ascii_case("ea_wrc") {
        "eawrc"
    } else if game_id.eq_ignore_ascii_case("f1_2025") {
        "f1_25"
    } else {
        game_id
    }
}

#[cfg(test)]
mod tests {
    use super::normalize_game_id;

    #[test]
    fn normalizes_known_aliases_case_insensitively() {
        assert_eq!(normalize_game_id("ea_wrc"), "eawrc");
        assert_eq!(normalize_game_id("EA_WRC"), "eawrc");
        assert_eq!(normalize_game_id("f1_2025"), "f1_25");
        assert_eq!(normalize_game_id("F1_2025"), "f1_25");
    }

    #[test]
    fn passes_through_non_alias_inputs() {
        assert_eq!(normalize_game_id("f1_25"), "f1_25");
        assert_eq!(normalize_game_id("iracing"), "iracing");
        assert_eq!(normalize_game_id(""), "");
    }
}
