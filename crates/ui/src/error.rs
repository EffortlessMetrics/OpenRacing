//! Error handling for OpenRacing UI
//!
//! This module provides user-friendly error message formatting for the Tauri UI.
//! It ensures that error messages displayed to users are:
//! - Non-empty and descriptive
//! - Free of internal implementation details (stack traces, file paths, etc.)
//! - Helpful for troubleshooting
//!
//! ## Requirements Coverage
//!
//! - **7.5**: WHEN an error occurs, THE Tauri_UI SHALL display a user-friendly error message

/// Patterns that indicate internal implementation details that should not be exposed to users
const INTERNAL_PATTERNS: &[&str] = &[
    // Stack trace indicators
    "at 0x",
    "stack backtrace",
    "RUST_BACKTRACE",
    "panicked at",
    "thread '",
    "note: run with",
    // Internal file paths (Unix)
    "/home/",
    "/usr/",
    "/var/",
    "/tmp/",
    "/root/",
    "/.cargo/",
    "/rustc/",
    // Internal file paths (Windows)
    "C:\\Users\\",
    "C:\\Program Files",
    "C:\\Windows",
    "\\AppData\\",
    "\\.cargo\\",
    // Source code references
    ".rs:",
    "src/",
    "crates/",
    // Memory addresses
    "0x7f",
    "0x00",
    // Internal error types
    "Box<dyn",
    "Arc<",
    "Mutex<",
    "RwLock<",
    // Debug formatting artifacts
    "{ ",
    " }",
    "Some(",
    "None",
    "Ok(",
    "Err(",
];

/// Maximum reasonable length for a user-facing error message
const MAX_ERROR_LENGTH: usize = 500;

/// Minimum length for a meaningful error message
const MIN_ERROR_LENGTH: usize = 5;

/// Checks if an error message contains internal implementation details
/// that should not be exposed to users.
///
/// Returns `true` if the message appears to contain internal details.
pub fn contains_internal_details(message: &str) -> bool {
    let lower = message.to_lowercase();

    for pattern in INTERNAL_PATTERNS {
        if lower.contains(&pattern.to_lowercase()) {
            return true;
        }
    }

    // Check for hex memory addresses (common in stack traces)
    if contains_hex_address(&lower) {
        return true;
    }

    false
}

/// Checks if the message contains what looks like a hex memory address
fn contains_hex_address(message: &str) -> bool {
    // Look for patterns like "0x7fff..." or "0x0000..."
    let bytes = message.as_bytes();
    for i in 0..bytes.len().saturating_sub(5) {
        if bytes[i] == b'0' && bytes[i + 1] == b'x' {
            // Check if followed by at least 4 hex digits
            let hex_count = bytes[i + 2..]
                .iter()
                .take(8)
                .take_while(|&&b| b.is_ascii_hexdigit())
                .count();
            if hex_count >= 4 {
                return true;
            }
        }
    }
    false
}

/// Validates that an error message is suitable for display to users.
///
/// A valid user-facing error message must:
/// - Be non-empty (at least MIN_ERROR_LENGTH characters)
/// - Not exceed MAX_ERROR_LENGTH characters
/// - Not contain internal implementation details
/// - Be printable ASCII or valid UTF-8
///
/// Returns `Ok(())` if the message is valid, or an error describing the issue.
pub fn validate_user_error_message(message: &str) -> Result<(), String> {
    // Check minimum length
    let trimmed = message.trim();
    if trimmed.len() < MIN_ERROR_LENGTH {
        return Err(format!(
            "Error message too short: {} chars (minimum {})",
            trimmed.len(),
            MIN_ERROR_LENGTH
        ));
    }

    // Check maximum length
    if message.len() > MAX_ERROR_LENGTH {
        return Err(format!(
            "Error message too long: {} chars (maximum {})",
            message.len(),
            MAX_ERROR_LENGTH
        ));
    }

    // Check for internal details
    if contains_internal_details(message) {
        return Err("Error message contains internal implementation details".to_string());
    }

    // Check for valid UTF-8 (already guaranteed by &str, but check for control chars)
    if message
        .chars()
        .any(|c| c.is_control() && c != '\n' && c != '\r' && c != '\t')
    {
        return Err("Error message contains invalid control characters".to_string());
    }

    Ok(())
}

/// Sanitizes an internal error message for user display.
///
/// This function takes a potentially internal error message and converts it
/// to a user-friendly format by:
/// - Removing stack traces
/// - Removing file paths
/// - Removing memory addresses
/// - Truncating overly long messages
/// - Ensuring the message is non-empty
///
/// # Arguments
///
/// * `internal_error` - The raw error message from internal systems
/// * `context` - Optional context to prepend (e.g., "Failed to connect")
///
/// # Returns
///
/// A sanitized, user-friendly error message
pub fn sanitize_error_message(internal_error: &str, context: Option<&str>) -> String {
    let mut result = String::new();

    // Add context if provided
    if let Some(ctx) = context {
        result.push_str(ctx);
        if !internal_error.is_empty() {
            result.push_str(": ");
        }
    }

    // Extract the meaningful part of the error
    let sanitized = extract_user_message(internal_error);
    result.push_str(&sanitized);

    // Ensure we have a meaningful message
    if result.trim().len() < MIN_ERROR_LENGTH {
        if let Some(ctx) = context {
            return format!("{}: An unexpected error occurred", ctx);
        }
        return "An unexpected error occurred".to_string();
    }

    // Truncate if too long
    if result.len() > MAX_ERROR_LENGTH {
        result.truncate(MAX_ERROR_LENGTH - 3);
        result.push_str("...");
    }

    result
}

/// Extracts the user-meaningful portion of an error message
fn extract_user_message(error: &str) -> String {
    // Split by common separators and take the first meaningful part
    let lines: Vec<&str> = error.lines().collect();

    // Find the first line that doesn't look like internal details
    for line in &lines {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !contains_internal_details(trimmed) {
            // Remove common prefixes like "Error: " or "error: "
            let cleaned = trimmed
                .strip_prefix("Error: ")
                .or_else(|| trimmed.strip_prefix("error: "))
                .or_else(|| trimmed.strip_prefix("ERROR: "))
                .unwrap_or(trimmed);

            if cleaned.len() >= MIN_ERROR_LENGTH {
                return cleaned.to_string();
            }
        }
    }

    // If no clean line found, try to extract from the first line
    if let Some(first) = lines.first() {
        let trimmed = first.trim();
        // Take up to the first colon or newline that might indicate internal details
        if let Some(pos) = trimmed.find(": /") {
            return trimmed[..pos].to_string();
        }
        if let Some(pos) = trimmed.find(" at ") {
            return trimmed[..pos].to_string();
        }
    }

    String::new()
}

/// Formats an IPC error for user display
///
/// This is the primary function used by Tauri commands to format errors
/// before returning them to the frontend.
pub fn format_ipc_error(operation: &str, error: impl std::fmt::Display) -> String {
    let error_str = error.to_string();
    sanitize_error_message(&error_str, Some(operation))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_contains_internal_details_stack_trace() {
        assert!(contains_internal_details("at 0x7fff12345678"));
        assert!(contains_internal_details("stack backtrace:"));
        assert!(contains_internal_details("panicked at 'error'"));
    }

    #[test]
    fn test_contains_internal_details_paths() {
        assert!(contains_internal_details("/home/user/project/src/main.rs"));
        assert!(contains_internal_details("C:\\Users\\Admin\\project"));
        assert!(contains_internal_details(
            "error in crates/ui/src/commands.rs:42"
        ));
    }

    #[test]
    fn test_contains_internal_details_clean_message() {
        assert!(!contains_internal_details("Failed to connect to service"));
        assert!(!contains_internal_details("Device not found"));
        assert!(!contains_internal_details(
            "Profile validation failed: invalid gain value"
        ));
    }

    #[test]
    fn test_validate_user_error_message_valid() -> Result<(), String> {
        validate_user_error_message("Failed to connect to the wheeld service")?;
        validate_user_error_message("Device 'Fanatec CSL DD' not found")?;
        validate_user_error_message(
            "Profile validation failed: FFB gain must be between 0 and 100",
        )?;
        Ok(())
    }

    #[test]
    fn test_validate_user_error_message_too_short() {
        let result = validate_user_error_message("Err");
        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|e| e.contains("too short"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_validate_user_error_message_internal_details() {
        let result = validate_user_error_message("Error at /home/user/project/src/main.rs:42");
        assert!(result.is_err());
        assert!(
            result
                .err()
                .map(|e| e.contains("internal"))
                .unwrap_or(false)
        );
    }

    #[test]
    fn test_sanitize_error_message() {
        let sanitized = sanitize_error_message(
            "connection refused at /home/user/.cargo/registry/src/tokio-1.0.0/src/net.rs:123",
            Some("Failed to connect"),
        );
        assert!(!sanitized.contains("/home/"));
        assert!(sanitized.starts_with("Failed to connect"));
    }

    #[test]
    fn test_format_ipc_error() {
        let formatted = format_ipc_error("list devices", "connection refused");
        assert!(formatted.contains("list devices"));
        assert!(!formatted.is_empty());
    }
}

#[cfg(test)]
mod property_tests {
    use super::*;
    use proptest::prelude::*;

    // Feature: release-roadmap-v1, Property 6: UI Error Display
    // **Validates: Requirements 7.5**
    //
    // For any error condition returned from the service layer, the Tauri UI
    // SHALL display a non-empty, user-readable error message.

    /// Strategy to generate realistic internal error messages that might come from
    /// various parts of the system (IPC, file I/O, network, etc.)
    fn internal_error_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            // Simple error messages
            "[a-zA-Z ]{10,50}".prop_map(|s| s),
            // Error with context
            "[a-zA-Z ]{5,20}: [a-zA-Z ]{10,30}".prop_map(|s| s),
            // Network errors
            Just("connection refused".to_string()),
            Just("connection timed out".to_string()),
            Just("host not found".to_string()),
            Just("network unreachable".to_string()),
            // File errors
            Just("file not found".to_string()),
            Just("permission denied".to_string()),
            Just("invalid file format".to_string()),
            // Service errors
            Just("service unavailable".to_string()),
            Just("request timeout".to_string()),
            Just("invalid response".to_string()),
            // Device errors
            Just("device not found".to_string()),
            Just("device disconnected".to_string()),
            Just("device busy".to_string()),
            // Profile errors
            Just("invalid profile format".to_string()),
            Just("profile not found".to_string()),
            Just("validation failed".to_string()),
        ]
    }

    /// Strategy to generate operation context strings
    fn operation_context_strategy() -> impl Strategy<Value = String> {
        prop_oneof![
            Just("Failed to connect to service".to_string()),
            Just("Failed to list devices".to_string()),
            Just("Failed to get device status".to_string()),
            Just("Failed to apply profile".to_string()),
            Just("Failed to read telemetry".to_string()),
            Just("Failed to execute emergency stop".to_string()),
            Just("Failed to load profile".to_string()),
            Just("Failed to save settings".to_string()),
        ]
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(100))]

        // Feature: release-roadmap-v1, Property 6: UI Error Display
        // **Validates: Requirements 7.5**
        //
        // Property: For any internal error message, the sanitized output SHALL be
        // non-empty and user-readable (no internal implementation details).
        #[test]
        fn prop_sanitized_errors_are_user_friendly(
            internal_error in internal_error_strategy(),
            context in operation_context_strategy(),
        ) {
            let sanitized = sanitize_error_message(&internal_error, Some(&context));

            // The sanitized message must be non-empty
            prop_assert!(
                !sanitized.trim().is_empty(),
                "Sanitized error message should not be empty. Input: '{}', Context: '{}'",
                internal_error,
                context
            );

            // The sanitized message must not contain internal details
            prop_assert!(
                !contains_internal_details(&sanitized),
                "Sanitized error should not contain internal details. Got: '{}'",
                sanitized
            );

            // The sanitized message must be within reasonable length bounds
            prop_assert!(
                sanitized.len() <= MAX_ERROR_LENGTH,
                "Sanitized error too long: {} chars",
                sanitized.len()
            );

            // The sanitized message should start with the context
            prop_assert!(
                sanitized.starts_with(&context),
                "Sanitized error should start with context. Got: '{}', Expected prefix: '{}'",
                sanitized,
                context
            );
        }

        // Feature: release-roadmap-v1, Property 6: UI Error Display
        // **Validates: Requirements 7.5**
        //
        // Property: For any formatted IPC error, the result SHALL be a valid
        // user-facing error message.
        #[test]
        fn prop_formatted_ipc_errors_are_valid(
            operation in "[a-zA-Z ]{5,30}",
            error_msg in internal_error_strategy(),
        ) {
            let formatted = format_ipc_error(&operation, &error_msg);

            // The formatted message must pass validation
            let validation_result = validate_user_error_message(&formatted);
            prop_assert!(
                validation_result.is_ok(),
                "Formatted IPC error should be valid. Got: '{}', Validation error: {:?}",
                formatted,
                validation_result.err()
            );
        }

        // Feature: release-roadmap-v1, Property 6: UI Error Display
        // **Validates: Requirements 7.5**
        //
        // Property: Error messages with internal paths SHALL have those paths removed
        // after sanitization.
        #[test]
        fn prop_internal_paths_are_removed(
            prefix in "[a-zA-Z ]{5,20}",
            path_type in prop_oneof![
                Just("/home/user/project/"),
                Just("/usr/local/lib/"),
                Just("C:\\Users\\Admin\\"),
                Just("/.cargo/registry/"),
            ],
            suffix in "[a-zA-Z0-9_.]{5,20}",
        ) {
            let internal_error = format!("{} at {}{}", prefix, path_type, suffix);
            let sanitized = sanitize_error_message(&internal_error, Some("Operation failed"));

            // The sanitized message should not contain the path
            prop_assert!(
                !sanitized.contains(path_type),
                "Sanitized error should not contain path '{}'. Got: '{}'",
                path_type,
                sanitized
            );
        }

        // Feature: release-roadmap-v1, Property 6: UI Error Display
        // **Validates: Requirements 7.5**
        //
        // Property: Error messages with stack traces SHALL have traces removed
        // after sanitization.
        #[test]
        fn prop_stack_traces_are_removed(
            error_prefix in "[a-zA-Z ]{10,30}",
            address in "[0-9a-f]{8,16}",
        ) {
            let internal_error = format!("{}\nstack backtrace:\n  0: 0x{}", error_prefix, address);
            let sanitized = sanitize_error_message(&internal_error, Some("Error occurred"));

            // The sanitized message should not contain stack trace indicators
            prop_assert!(
                !sanitized.contains("stack backtrace"),
                "Sanitized error should not contain stack trace. Got: '{}'",
                sanitized
            );
            prop_assert!(
                !sanitized.contains(&format!("0x{}", address)),
                "Sanitized error should not contain memory address. Got: '{}'",
                sanitized
            );
        }

        // Feature: release-roadmap-v1, Property 6: UI Error Display
        // **Validates: Requirements 7.5**
        //
        // Property: Any valid user error message SHALL pass validation.
        #[test]
        fn prop_valid_messages_pass_validation(
            message in "[A-Za-z][A-Za-z0-9 ,.!?'-]{10,100}",
        ) {
            // Messages that don't contain internal patterns should pass validation
            if !contains_internal_details(&message) {
                let result = validate_user_error_message(&message);
                prop_assert!(
                    result.is_ok(),
                    "Valid message should pass validation. Message: '{}', Error: {:?}",
                    message,
                    result.err()
                );
            }
        }

        // Feature: release-roadmap-v1, Property 6: UI Error Display
        // **Validates: Requirements 7.5**
        //
        // Property: Empty or very short error messages SHALL be replaced with
        // a meaningful default message after sanitization.
        #[test]
        fn prop_empty_errors_get_default_message(
            short_error in "[a-z]{0,4}",
            context in operation_context_strategy(),
        ) {
            let sanitized = sanitize_error_message(&short_error, Some(&context));

            // The sanitized message must be non-empty and meaningful
            prop_assert!(
                sanitized.trim().len() >= MIN_ERROR_LENGTH,
                "Sanitized error should have minimum length. Got: '{}' ({} chars)",
                sanitized,
                sanitized.trim().len()
            );
        }
    }
}
