//! Integration test demonstrating the IPC conversion layer
//!
//! This test shows that the conversion layer successfully separates
//! domain logic from wire protocol concerns.

#[cfg(test)]
mod tests {
    use crate::domain::*;
    use crate::entities::*;
    use crate::telemetry::TelemetryData;
    use crate::generated::wheel::v1 as proto;
    use crate::ipc_conversion::ConversionError;

    /// Test that demonstrates a complete service workflow using conversions
    #[test]
    fn test_service_workflow_with_conversions() {
        // 1. Create domain objects (what the service layer works with)
        let device_id: DeviceId = "wheel-base-001".parse().unwrap();
        
        let capabilities = DeviceCapabilities::new(
            true,  // supports_pid
            true,  // supports_raw_torque_1khz
            true,  // supports_health_stream
            false, // supports_led_bus
            TorqueNm::new(25.0).unwrap(),
            10000, // encoder_cpr
            1000,  // min_report_period_us (1kHz)
        );
        
        let device = Device::new(
            device_id.clone(),
            "Fanatec CSL DD".to_string(),
            DeviceType::WheelBase,
            capabilities,
        );
        
        let telemetry = TelemetryData {
            wheel_angle_deg: 180.5,
            wheel_speed_rad_s: 3.14159,
            temperature_c: 65,
            fault_flags: 0b00000001, // Single fault bit set
            hands_on: true,
            timestamp: 1234567890,
        };
        
        // 2. Convert domain objects to wire format (for IPC transmission)
        let wire_device: proto::DeviceInfo = device.clone().into();
        let wire_telemetry: proto::TelemetryData = telemetry.clone().into();
        
        // Verify wire format has correct values
        assert_eq!(wire_device.id, "wheel-base-001");
        assert_eq!(wire_device.name, "Fanatec CSL DD");
        assert_eq!(wire_device.r#type, 1); // WheelBase = 1
        assert_eq!(wire_device.state, 1);  // Connected = 1
        
        let wire_caps = wire_device.capabilities.unwrap();
        assert_eq!(wire_caps.max_torque_cnm, 2500); // 25.0 Nm = 2500 cNm
        assert_eq!(wire_caps.encoder_cpr, 10000);
        assert_eq!(wire_caps.min_report_period_us, 1000);
        
        // Verify telemetry unit conversions
        assert_eq!(wire_telemetry.wheel_angle_mdeg, 180500); // 180.5° = 180500 mdeg
        assert_eq!(wire_telemetry.wheel_speed_mrad_s, 3142); // 3.14159 rad/s ≈ 3142 mrad/s
        assert_eq!(wire_telemetry.temp_c, 65);
        assert_eq!(wire_telemetry.faults, 1);
        assert_eq!(wire_telemetry.hands_on, true);
        assert_eq!(wire_telemetry.sequence, 0); // Deprecated field
        
        // 3. Convert wire format back to domain objects (after IPC reception)
        let received_device: Device = wire_device.try_into().unwrap();
        let received_telemetry: TelemetryData = wire_telemetry.try_into().unwrap();
        
        // Verify domain objects are correctly reconstructed
        assert_eq!(received_device.id.as_str(), "wheel-base-001");
        assert_eq!(received_device.name, "Fanatec CSL DD");
        assert_eq!(received_device.device_type, DeviceType::WheelBase);
        assert_eq!(received_device.capabilities.max_torque.value(), 25.0);
        
        // Verify telemetry precision is preserved within acceptable tolerance
        assert!((received_telemetry.wheel_angle_deg - 180.5).abs() < 0.001);
        assert!((received_telemetry.wheel_speed_rad_s - 3.14159).abs() < 0.001);
        assert_eq!(received_telemetry.temperature_c, 65);
        assert_eq!(received_telemetry.fault_flags, 1);
        assert_eq!(received_telemetry.hands_on, true);
    }
    
    /// Test that demonstrates profile conversion with validation
    #[test]
    fn test_profile_conversion_with_validation() {
        // Create a domain profile
        let profile_id: ProfileId = "iracing-gt3".parse().unwrap();
        let base_settings = BaseSettings::new(
            Gain::new(0.85).unwrap(),
            Degrees::new_dor(540.0).unwrap(),
            TorqueNm::new(20.0).unwrap(),
            FilterConfig::default(),
        );
        
        let profile = Profile::new(
            profile_id,
            ProfileScope::for_game("iRacing".to_string()),
            base_settings,
            "iRacing GT3 Profile".to_string(),
        );
        
        // Convert to wire format
        let wire_profile: proto::Profile = profile.into();
        
        // Verify wire format
        assert_eq!(wire_profile.schema_version, "wheel.profile/1");
        assert_eq!(wire_profile.scope.as_ref().unwrap().game, "iRacing");
        assert_eq!(wire_profile.base.as_ref().unwrap().ffb_gain, 0.85);
        assert_eq!(wire_profile.base.as_ref().unwrap().dor_deg, 540);
        assert_eq!(wire_profile.base.as_ref().unwrap().torque_cap_nm, 20.0);
        
        // Convert back to domain
        let received_profile: Profile = wire_profile.try_into().unwrap();
        
        // Verify domain reconstruction
        assert_eq!(received_profile.base_settings.ffb_gain.value(), 0.85);
        assert_eq!(received_profile.base_settings.degrees_of_rotation.value(), 540.0);
        assert_eq!(received_profile.base_settings.torque_cap.value(), 20.0);
        assert_eq!(received_profile.scope.game, Some("iRacing".to_string()));
    }
    
    /// Test that demonstrates validation errors are properly handled
    #[test]
    fn test_validation_error_handling() {
        // Create invalid wire telemetry (temperature too high)
        let invalid_telemetry = proto::TelemetryData {
            wheel_angle_mdeg: 0,
            wheel_speed_mrad_s: 0,
            temp_c: 200, // Invalid: > 150°C
            faults: 0,
            hands_on: false,
            sequence: 0,
        };
        
        // Conversion should fail with validation error
        let result: Result<TelemetryData, ConversionError> = invalid_telemetry.try_into();
        assert!(result.is_err());
        
        match result.unwrap_err() {
            ConversionError::RangeValidation { field, value, min, max } => {
                assert_eq!(field, "temperature_c");
                assert_eq!(value, 200.0);
                assert_eq!(min, 0.0);
                assert_eq!(max, 150.0);
            }
            _ => panic!("Expected RangeValidation error"),
        }
        
        // Create invalid wire profile (gain too high)
        let invalid_profile = proto::BaseSettings {
            ffb_gain: 1.5, // Invalid: > 1.0
            dor_deg: 900,
            torque_cap_nm: 15.0,
            filters: Some(proto::FilterConfig {
                reconstruction: 4,
                friction: 0.1,
                damper: 0.1,
                inertia: 0.1,
                notch_filters: vec![],
                slew_rate: 0.8,
                curve_points: vec![
                    proto::CurvePoint { input: 0.0, output: 0.0 },
                    proto::CurvePoint { input: 1.0, output: 1.0 },
                ],
            }),
        };
        
        // Conversion should fail with domain validation error
        let result: Result<BaseSettings, ConversionError> = invalid_profile.try_into();
        assert!(result.is_err());
        
        match result.unwrap_err() {
            ConversionError::DomainError(domain_error) => {
                // Should be InvalidGain error
                assert!(domain_error.to_string().contains("Invalid gain"));
            }
            _ => panic!("Expected DomainError"),
        }
    }
    
    /// Test that demonstrates the service layer only needs to work with domain types
    #[test]
    fn test_service_layer_isolation() {
        // This test shows that a service method can work purely with domain types
        // and the conversion layer handles all wire protocol concerns
        
        fn mock_service_method(device_id: &DeviceId) -> Result<(Device, TelemetryData), String> {
            // Service layer works only with domain types
            let capabilities = DeviceCapabilities::new(
                true, true, true, false,
                TorqueNm::new(15.0).unwrap(),
                8192,
                1000,
            );
            
            let device = Device::new(
                device_id.clone(),
                "Mock Device".to_string(),
                DeviceType::WheelBase,
                capabilities,
            );
            
            let telemetry = TelemetryData {
                wheel_angle_deg: 45.0,
                wheel_speed_rad_s: 1.0,
                temperature_c: 50,
                fault_flags: 0,
                hands_on: true,
                timestamp: 1000,
            };
            
            Ok((device, telemetry))
        }
        
        // Service call with domain types
        let device_id: DeviceId = "test-device".parse().unwrap();
        let (device, telemetry) = mock_service_method(&device_id).unwrap();
        
        // IPC layer converts to wire format for transmission
        let wire_device: proto::DeviceInfo = device.into();
        let wire_telemetry: proto::TelemetryData = telemetry.into();
        
        // Verify conversion worked
        assert_eq!(wire_device.id, "test-device");
        assert_eq!(wire_device.name, "Mock Device");
        assert_eq!(wire_telemetry.wheel_angle_mdeg, 45000);
        assert_eq!(wire_telemetry.wheel_speed_mrad_s, 1000);
        
        // Client receives wire format and converts back to domain types
        let client_device: Device = wire_device.try_into().unwrap();
        let client_telemetry: TelemetryData = wire_telemetry.try_into().unwrap();
        
        // Client now has domain types to work with
        assert_eq!(client_device.id.as_str(), "test-device");
        assert_eq!(client_telemetry.wheel_angle_deg, 45.0);
        assert_eq!(client_telemetry.wheel_speed_rad_s, 1.0);
    }
}