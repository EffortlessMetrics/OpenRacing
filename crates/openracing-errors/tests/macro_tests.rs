#![allow(clippy::manual_range_contains, clippy::double_comparisons)]
//! Tests for the error macros: error_context!, validate!, validate_range!, require!.

use openracing_errors::{
    OpenRacingError, ValidationError, error_context, require, validate, validate_range,
};

// ---------------------------------------------------------------------------
// error_context! macro
// ---------------------------------------------------------------------------

mod error_context_macro {
    use super::*;

    #[test]
    fn no_key_value_pairs() -> Result<(), OpenRacingError> {
        let ctx = error_context!("my_operation",);
        let display = ctx.to_string();
        assert!(display.contains("my_operation"));
        Ok(())
    }

    #[test]
    fn single_key_value_pair() -> Result<(), OpenRacingError> {
        let ctx = error_context!("save_profile", "id" => "gt3");
        let display = ctx.to_string();
        assert!(display.contains("save_profile"));
        assert!(display.contains("id"));
        assert!(display.contains("gt3"));
        Ok(())
    }

    #[test]
    fn multiple_key_value_pairs() -> Result<(), OpenRacingError> {
        let ctx = error_context!(
            "apply_settings",
            "profile" => "gt3",
            "device" => "moza-r9",
            "gain" => "0.8"
        );
        let display = ctx.to_string();
        assert!(display.contains("apply_settings"));
        assert!(display.contains("profile"));
        assert!(display.contains("gt3"));
        assert!(display.contains("device"));
        assert!(display.contains("moza-r9"));
        assert!(display.contains("gain"));
        assert!(display.contains("0.8"));
        Ok(())
    }

    #[test]
    fn trailing_comma() -> Result<(), OpenRacingError> {
        let ctx = error_context!(
            "init",
            "stage" => "bootstrap",
        );
        let display = ctx.to_string();
        assert!(display.contains("init"));
        assert!(display.contains("bootstrap"));
        Ok(())
    }

    #[test]
    fn empty_operation_string() -> Result<(), OpenRacingError> {
        let ctx = error_context!("",);
        // Should not panic, just produce some output
        let _display = ctx.to_string();
        Ok(())
    }

    #[test]
    fn empty_key_value() -> Result<(), OpenRacingError> {
        let ctx = error_context!("op", "" => "");
        let _display = ctx.to_string();
        Ok(())
    }

    #[test]
    fn special_characters_in_operation() -> Result<(), OpenRacingError> {
        let ctx = error_context!("load/save [profile]", "path" => "C:\\users\\test");
        let display = ctx.to_string();
        assert!(display.contains("load/save [profile]"));
        assert!(display.contains("C:\\users\\test"));
        Ok(())
    }

    #[test]
    fn unicode_in_context() -> Result<(), OpenRacingError> {
        let ctx = error_context!("处理", "状态" => "错误");
        let display = ctx.to_string();
        assert!(display.contains("处理"));
        assert!(display.contains("错误"));
        Ok(())
    }

    #[test]
    fn context_with_numeric_values_as_strings() -> Result<(), OpenRacingError> {
        let ctx = error_context!("tick_process", "tick" => "12345", "latency_us" => "250");
        let display = ctx.to_string();
        assert!(display.contains("12345"));
        assert!(display.contains("250"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// validate! macro
// ---------------------------------------------------------------------------

mod validate_macro {
    use super::*;

    fn check_nonempty(s: &str) -> Result<(), OpenRacingError> {
        validate!(!s.is_empty(), ValidationError::required("input"));
        Ok(())
    }

    #[test]
    fn passing_condition() -> Result<(), OpenRacingError> {
        let result = check_nonempty("hello");
        assert!(result.is_ok());
        Ok(())
    }

    #[test]
    fn failing_condition() -> Result<(), OpenRacingError> {
        let result = check_nonempty("");
        assert!(result.is_err());
        if let Err(OpenRacingError::Validation(ValidationError::Required(field))) = result {
            assert_eq!(field, "input");
        } else {
            panic!("Expected ValidationError::Required");
        }
        Ok(())
    }

    #[test]
    fn validate_with_custom_error() -> Result<(), OpenRacingError> {
        fn check_positive(v: i32) -> Result<(), OpenRacingError> {
            validate!(v > 0, ValidationError::custom("value must be positive"));
            Ok(())
        }
        assert!(check_positive(1).is_ok());
        assert!(check_positive(0).is_err());
        assert!(check_positive(-5).is_err());
        Ok(())
    }

    #[test]
    fn validate_with_constraint_error() -> Result<(), OpenRacingError> {
        fn check_even(v: i32) -> Result<(), OpenRacingError> {
            validate!(
                v % 2 == 0,
                ValidationError::constraint("value must be even")
            );
            Ok(())
        }
        assert!(check_even(4).is_ok());
        assert!(check_even(3).is_err());
        Ok(())
    }

    #[test]
    fn validate_with_rt_error() -> Result<(), OpenRacingError> {
        use openracing_errors::RTError;

        fn check_config_valid(valid: bool) -> Result<(), OpenRacingError> {
            validate!(valid, RTError::InvalidConfig);
            Ok(())
        }
        assert!(check_config_valid(true).is_ok());
        let err = check_config_valid(false).unwrap_err();
        assert!(matches!(err, OpenRacingError::RT(RTError::InvalidConfig)));
        Ok(())
    }

    #[test]
    fn validate_true_does_not_short_circuit() -> Result<(), OpenRacingError> {
        fn multi_validate() -> Result<i32, OpenRacingError> {
            validate!(true, ValidationError::required("a"));
            validate!(true, ValidationError::required("b"));
            Ok(42)
        }
        let result = multi_validate()?;
        assert_eq!(result, 42);
        Ok(())
    }

    #[test]
    fn validate_false_short_circuits_early() -> Result<(), OpenRacingError> {
        fn multi_validate() -> Result<i32, OpenRacingError> {
            validate!(false, ValidationError::required("first"));
            // This line should never be reached
            validate!(false, ValidationError::required("second"));
            Ok(42)
        }
        let err = multi_validate().unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("first"));
        assert!(!msg.contains("second"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// validate_range! macro
// ---------------------------------------------------------------------------

mod validate_range_macro {
    use super::*;

    fn check_gain(gain: f32) -> Result<(), OpenRacingError> {
        validate_range!("gain", gain, 0.0_f32, 1.0_f32);
        Ok(())
    }

    #[test]
    fn value_within_range() -> Result<(), OpenRacingError> {
        assert!(check_gain(0.5).is_ok());
        Ok(())
    }

    #[test]
    fn value_at_lower_bound() -> Result<(), OpenRacingError> {
        assert!(check_gain(0.0).is_ok());
        Ok(())
    }

    #[test]
    fn value_at_upper_bound() -> Result<(), OpenRacingError> {
        assert!(check_gain(1.0).is_ok());
        Ok(())
    }

    #[test]
    fn value_below_lower_bound() -> Result<(), OpenRacingError> {
        let result = check_gain(-0.1);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("gain"));
        Ok(())
    }

    #[test]
    fn value_above_upper_bound() -> Result<(), OpenRacingError> {
        let result = check_gain(1.1);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(msg.contains("gain"));
        Ok(())
    }

    #[test]
    fn integer_range() -> Result<(), OpenRacingError> {
        fn check_port(port: u16) -> Result<(), OpenRacingError> {
            validate_range!("port", port, 1024_u16, 65535_u16);
            Ok(())
        }
        assert!(check_port(8080).is_ok());
        assert!(check_port(1024).is_ok());
        assert!(check_port(65535).is_ok());
        assert!(check_port(80).is_err());
        Ok(())
    }

    #[test]
    fn signed_integer_range() -> Result<(), OpenRacingError> {
        fn check_temp(temp: i32) -> Result<(), OpenRacingError> {
            validate_range!("temperature", temp, -40_i32, 85_i32);
            Ok(())
        }
        assert!(check_temp(20).is_ok());
        assert!(check_temp(-40).is_ok());
        assert!(check_temp(85).is_ok());
        assert!(check_temp(-41).is_err());
        assert!(check_temp(86).is_err());
        Ok(())
    }

    #[test]
    fn negative_float_range() -> Result<(), OpenRacingError> {
        fn check_torque(t: f64) -> Result<(), OpenRacingError> {
            validate_range!("torque", t, -1.0_f64, 1.0_f64);
            Ok(())
        }
        assert!(check_torque(0.0).is_ok());
        assert!(check_torque(-1.0).is_ok());
        assert!(check_torque(1.0).is_ok());
        assert!(check_torque(-1.01).is_err());
        assert!(check_torque(1.01).is_err());
        Ok(())
    }

    #[test]
    fn error_message_contains_field_name() -> Result<(), OpenRacingError> {
        fn check(v: f32) -> Result<(), OpenRacingError> {
            validate_range!("my_field", v, 0.0_f32, 10.0_f32);
            Ok(())
        }
        let err = check(20.0).unwrap_err();
        assert!(err.to_string().contains("my_field"));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// require! macro
// ---------------------------------------------------------------------------

mod require_macro {
    use super::*;

    #[test]
    fn require_creates_required_error() -> Result<(), OpenRacingError> {
        let err = require!("profile_name");
        assert_eq!(
            err.to_string(),
            "Required field 'profile_name' is missing"
        );
        Ok(())
    }

    #[test]
    fn require_different_field_names() -> Result<(), OpenRacingError> {
        let fields = ["name", "id", "device_id", "path"];
        for field in fields {
            let err = require!(field);
            let msg = err.to_string();
            assert!(msg.contains(field));
            assert!(msg.contains("Required"));
        }
        Ok(())
    }

    #[test]
    fn require_with_string_expression() -> Result<(), OpenRacingError> {
        let field_name = String::from("dynamic_field");
        let err = require!(&field_name);
        assert!(err.to_string().contains("dynamic_field"));
        Ok(())
    }

    #[test]
    fn require_converts_to_openracing_error() -> Result<(), OpenRacingError> {
        let validation_err = require!("field");
        let err: OpenRacingError = validation_err.into();
        assert!(matches!(
            err,
            OpenRacingError::Validation(ValidationError::Required(_))
        ));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Edge cases: empty strings, special characters
// ---------------------------------------------------------------------------

mod edge_cases {
    use super::*;

    #[test]
    fn empty_string_in_error_context() -> Result<(), OpenRacingError> {
        let ctx = error_context!("", "" => "");
        let _display = ctx.to_string();
        Ok(())
    }

    #[test]
    fn special_chars_in_field_names() -> Result<(), OpenRacingError> {
        let err = ValidationError::required("field.name[0]");
        assert!(err.to_string().contains("field.name[0]"));
        Ok(())
    }

    #[test]
    fn newlines_in_context_values() -> Result<(), OpenRacingError> {
        let ctx = error_context!("op", "msg" => "line1\nline2\nline3");
        let display = ctx.to_string();
        assert!(display.contains("line1\nline2\nline3"));
        Ok(())
    }

    #[test]
    fn very_long_field_name() -> Result<(), OpenRacingError> {
        let long_name = "a".repeat(1000);
        let err = ValidationError::required(&long_name);
        let msg = err.to_string();
        assert!(msg.contains(&long_name));
        Ok(())
    }

    #[test]
    fn quotes_in_strings() -> Result<(), OpenRacingError> {
        let ctx = error_context!("op", "key" => r#"value with "quotes""#);
        let display = ctx.to_string();
        assert!(display.contains("quotes"));
        Ok(())
    }

    #[test]
    fn backslashes_in_paths() -> Result<(), OpenRacingError> {
        let ctx = error_context!("load", "path" => r"C:\Users\test\profile.yaml");
        let display = ctx.to_string();
        assert!(display.contains(r"C:\Users\test\profile.yaml"));
        Ok(())
    }

    #[test]
    fn null_bytes_in_strings() -> Result<(), OpenRacingError> {
        let ctx = error_context!("op", "key" => "before\0after");
        let _display = ctx.to_string();
        Ok(())
    }

    #[test]
    fn emoji_in_context() -> Result<(), OpenRacingError> {
        let ctx = error_context!("🏎️ race", "status" => "🔴 failed");
        let display = ctx.to_string();
        assert!(display.contains("🏎️ race"));
        assert!(display.contains("🔴 failed"));
        Ok(())
    }

    #[test]
    fn validate_range_with_equal_bounds() -> Result<(), OpenRacingError> {
        fn check(v: f32) -> Result<(), OpenRacingError> {
            validate_range!("exact", v, 5.0_f32, 5.0_f32);
            Ok(())
        }
        assert!(check(5.0).is_ok());
        assert!(check(5.1).is_err());
        assert!(check(4.9).is_err());
        Ok(())
    }

    #[test]
    fn error_context_no_pairs_no_trailing_comma() -> Result<(), OpenRacingError> {
        let ctx = error_context!("bare_op",);
        assert!(ctx.to_string().contains("bare_op"));
        Ok(())
    }
}
