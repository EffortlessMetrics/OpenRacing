use std::path::{Path, PathBuf};

pub(crate) fn resolve_game_path(game_path: &Path, relative_path: &str) -> PathBuf {
    // If a non-empty game_path is provided, respect it.
    // This is critical for tests using TempDir to avoid overwriting real user files.
    if !game_path.as_os_str().is_empty() && game_path != Path::new(".") {
        return game_path.join(relative_path);
    }

    #[cfg(windows)]
    if let Some(stripped) = relative_path.strip_prefix("Documents/") {
        // Try to use USERPROFILE/Documents as the base on Windows
        if let Some(user_profile) = std::env::var_os("USERPROFILE") {
            let mut path = PathBuf::from(user_profile);
            path.push("Documents");
            return path.join(stripped.replace('/', "\\"));
        }
    }
    game_path.join(relative_path)
}
