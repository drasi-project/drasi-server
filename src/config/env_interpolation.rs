// Copyright 2025 The Drasi Authors.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Environment variable interpolation for configuration files.
//!
//! This module provides transparent environment variable interpolation in YAML/JSON
//! configuration strings using POSIX-style syntax:
//! - `${VAR_NAME}` - Simple variable substitution
//! - `${VAR_NAME:-default}` - Variable with default value if unset/empty
//!
//! # Examples
//!
//! ```
//! use drasi_server::config::env_interpolation::interpolate;
//! use std::env;
//!
//! env::set_var("DB_HOST", "localhost");
//! env::set_var("DB_PORT", "5432");
//!
//! let input = r#"
//! host: ${DB_HOST}
//! port: ${DB_PORT}
//! database: ${DB_NAME:-mydb}
//! "#;
//!
//! let result = interpolate(input).unwrap();
//! assert!(result.contains("host: localhost"));
//! assert!(result.contains("port: 5432"));
//! assert!(result.contains("database: mydb"));
//! ```

use lazy_static::lazy_static;
use log::debug;
use regex::Regex;
use std::env;

/// Maximum length for interpolated strings to prevent DoS attacks
const MAX_INTERPOLATED_LENGTH: usize = 10_000_000; // 10MB

lazy_static! {
    /// Regex pattern for matching environment variable references.
    /// Captures:
    /// - Group 1: Variable name (must follow POSIX naming: [A-Za-z_][A-Za-z0-9_]*)
    /// - Group 2: Full default syntax (:-default) if present
    /// - Group 3: Default value (everything after :-) if present
    static ref ENV_VAR_PATTERN: Regex = Regex::new(
        r"\$\{([A-Za-z_][A-Za-z0-9_]*)(:-([^}]*))?\}"
    ).expect("Invalid regex pattern");
}

/// Errors that can occur during environment variable interpolation.
#[derive(Debug, thiserror::Error)]
pub enum InterpolationError {
    #[error("Environment variable '{name}' is not set and has no default value")]
    MissingVariable { name: String },

    #[error("Interpolated result exceeds maximum allowed length of {MAX_INTERPOLATED_LENGTH} bytes")]
    ResultTooLarge,
}

/// Interpolate environment variables in the input string.
///
/// Replaces all occurrences of `${VAR_NAME}` with the value of the environment
/// variable `VAR_NAME`. If the variable is not set, returns an error.
///
/// Supports default values: `${VAR_NAME:-default}` will use "default" if
/// `VAR_NAME` is not set or is empty.
///
/// # Arguments
///
/// * `input` - The string containing environment variable references
///
/// # Returns
///
/// The interpolated string with all variables replaced, or an error if:
/// - A required variable is missing
/// - The result would exceed the maximum size limit
///
/// # Security
///
/// - Only processes well-formed `${...}` patterns
/// - Does not execute code or allow recursive expansion
/// - Limits result size to prevent DoS attacks
/// - Variable names must follow POSIX rules: start with letter or underscore,
///   contain only letters, digits, and underscores
///
/// # Examples
///
/// ```
/// use drasi_server::config::env_interpolation::interpolate;
/// use std::env;
///
/// env::set_var("API_KEY", "secret123");
///
/// let result = interpolate("api_key: ${API_KEY}").unwrap();
/// assert_eq!(result, "api_key: secret123");
/// ```
pub fn interpolate(input: &str) -> Result<String, InterpolationError> {
    let mut result = String::with_capacity(input.len());
    let mut last_match_end = 0;
    let mut variables_used = Vec::new();

    for caps in ENV_VAR_PATTERN.captures_iter(input) {
        let full_match = caps.get(0).unwrap();
        let var_name = caps.get(1).unwrap().as_str();
        let default_value = caps.get(3).map(|m| m.as_str());

        // Add the text before this match
        result.push_str(&input[last_match_end..full_match.start()]);

        // Look up the environment variable
        let value = match env::var(var_name) {
            Ok(val) if !val.is_empty() => val,
            Ok(_) | Err(env::VarError::NotPresent) => {
                // Variable not set or is empty, use default if available
                match default_value {
                    Some(default) => default.to_string(),
                    None => {
                        return Err(InterpolationError::MissingVariable {
                            name: var_name.to_string(),
                        });
                    }
                }
            }
            Err(env::VarError::NotUnicode(_)) => {
                return Err(InterpolationError::MissingVariable {
                    name: format!("{var_name} (contains invalid Unicode)"),
                });
            }
        };

        variables_used.push(var_name);
        result.push_str(&value);
        last_match_end = full_match.end();

        // Check size limit
        if result.len() > MAX_INTERPOLATED_LENGTH {
            return Err(InterpolationError::ResultTooLarge);
        }
    }

    // Add any remaining text after the last match
    result.push_str(&input[last_match_end..]);

    // Log which variables were interpolated (names only, not values)
    if !variables_used.is_empty() {
        debug!(
            "Interpolated environment variables: {}",
            variables_used.join(", ")
        );
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_interpolation() {
        env::set_var("TEST_VAR1", "value1");
        env::set_var("TEST_VAR2", "value2");

        let input = "key1: ${TEST_VAR1}\nkey2: ${TEST_VAR2}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "key1: value1\nkey2: value2");
    }

    #[test]
    fn test_default_value_when_var_not_set() {
        env::remove_var("TEST_NONEXISTENT");

        let input = "value: ${TEST_NONEXISTENT:-default_value}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "value: default_value");
    }

    #[test]
    fn test_default_value_when_var_is_empty() {
        env::set_var("TEST_EMPTY", "");

        let input = "value: ${TEST_EMPTY:-default_value}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "value: default_value");
    }

    #[test]
    fn test_variable_value_overrides_default() {
        env::set_var("TEST_WITH_DEFAULT", "actual_value");

        let input = "value: ${TEST_WITH_DEFAULT:-default_value}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "value: actual_value");
    }

    #[test]
    fn test_missing_variable_without_default() {
        env::remove_var("TEST_MISSING");

        let input = "value: ${TEST_MISSING}";
        let result = interpolate(input);

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(InterpolationError::MissingVariable { .. })
        ));
    }

    #[test]
    fn test_multiple_variables_in_same_string() {
        env::set_var("TEST_HOST", "localhost");
        env::set_var("TEST_PORT", "8080");

        let input = "url: http://${TEST_HOST}:${TEST_PORT}/api";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "url: http://localhost:8080/api");
    }

    #[test]
    fn test_empty_default_value() {
        env::remove_var("TEST_EMPTY_DEFAULT");

        let input = "value: ${TEST_EMPTY_DEFAULT:-}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "value: ");
    }

    #[test]
    fn test_no_variables_returns_unchanged() {
        let input = "plain: text\nwith: no variables";
        let result = interpolate(input).unwrap();

        assert_eq!(result, input);
    }

    #[test]
    fn test_valid_variable_names() {
        env::set_var("_UNDERSCORE", "v1");
        env::set_var("CAPS123", "v2");
        env::set_var("lower_case_123", "v3");

        let input = "${_UNDERSCORE} ${CAPS123} ${lower_case_123}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "v1 v2 v3");
    }

    #[test]
    fn test_invalid_variable_name_with_dash() {
        // Dashes are not valid in POSIX variable names
        // This should NOT match the pattern
        let input = "value: ${INVALID-NAME}";
        let result = interpolate(input).unwrap();

        // Should remain unchanged because pattern doesn't match
        assert_eq!(result, input);
    }

    #[test]
    fn test_nested_braces_not_supported() {
        env::set_var("TEST_OUTER", "outer");
        env::set_var("TEST_INNER", "inner");

        // Nested variables are not supported - the regex will match TEST_INNER
        // but not process the outer ${TEST_...} pattern correctly
        let input = "${TEST_${TEST_INNER}}";
        let result = interpolate(input).unwrap();

        // The inner ${TEST_INNER} gets replaced with "inner"
        // but the outer pattern doesn't match our regex
        assert_eq!(result, "${TEST_inner}");
    }

    #[test]
    fn test_special_characters_in_values() {
        env::set_var("TEST_SPECIAL", "value with spaces & symbols! @#$%");

        let input = "key: ${TEST_SPECIAL}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "key: value with spaces & symbols! @#$%");
    }

    #[test]
    fn test_default_with_special_characters() {
        env::remove_var("TEST_SPECIAL_DEFAULT");

        let input = "key: ${TEST_SPECIAL_DEFAULT:-default with spaces!@#}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "key: default with spaces!@#");
    }

    #[test]
    fn test_multiline_yaml() {
        env::set_var("TEST_DB_HOST", "db.example.com");
        env::set_var("TEST_DB_PORT", "5432");

        let input = r#"
database:
  host: ${TEST_DB_HOST}
  port: ${TEST_DB_PORT}
  name: ${TEST_DB_NAME:-mydb}
"#;
        let result = interpolate(input).unwrap();

        assert!(result.contains("host: db.example.com"));
        assert!(result.contains("port: 5432"));
        assert!(result.contains("name: mydb"));
    }

    #[test]
    fn test_variable_in_quoted_string() {
        env::set_var("TEST_PASSWORD", "secret123");

        let input = r#"password: "${TEST_PASSWORD}""#;
        let result = interpolate(input).unwrap();

        assert_eq!(result, r#"password: "secret123""#);
    }

    #[test]
    fn test_unicode_in_values() {
        env::set_var("TEST_UNICODE", "Hello 世界 🌍");

        let input = "message: ${TEST_UNICODE}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "message: Hello 世界 🌍");
    }

    #[test]
    fn test_dos_protection_max_length() {
        // Create a very long value
        let long_value = "x".repeat(MAX_INTERPOLATED_LENGTH + 1);
        env::set_var("TEST_VERY_LONG", &long_value);

        let input = "${TEST_VERY_LONG}";
        let result = interpolate(input);

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(InterpolationError::ResultTooLarge)
        ));
    }

    #[test]
    fn test_numeric_values() {
        env::set_var("TEST_PORT", "8080");
        env::set_var("TEST_TIMEOUT", "30");

        let input = "port: ${TEST_PORT}\ntimeout: ${TEST_TIMEOUT}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "port: 8080\ntimeout: 30");
    }

    #[test]
    fn test_boolean_values() {
        env::set_var("TEST_ENABLED", "true");
        env::set_var("TEST_DEBUG", "false");

        let input = "enabled: ${TEST_ENABLED}\ndebug: ${TEST_DEBUG}";
        let result = interpolate(input).unwrap();

        assert_eq!(result, "enabled: true\ndebug: false");
    }
}
