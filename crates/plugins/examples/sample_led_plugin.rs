//! Sample LED mapping plugin

use racing_wheel_plugins::sdk::*;
use serde_json::Value;

/// Sample LED plugin that creates RPM-based patterns
#[derive(Default)]
pub struct SampleLedPlugin {
    max_rpm: f32,
    shift_point: f32,
}

impl WasmPlugin for SampleLedPlugin {
    fn initialize(&mut self, config: Value) -> SdkResult<()> {
        self.max_rpm = config
            .get("max_rpm")
            .and_then(|v| v.as_f64())
            .unwrap_or(8000.0) as f32;
        
        self.shift_point = config
            .get("shift_point")
            .and_then(|v| v.as_f64())
            .unwrap_or(0.9) as f32;
        
        Ok(())
    }
    
    fn process_telemetry(&mut self, _input: SdkTelemetry, _context: SdkContext) -> SdkResult<SdkOutput> {
        Err(SdkError::CapabilityRequired("ReadTelemetry".to_string()))
    }
    
    fn process_led_mapping(&mut self, input: SdkLedInput, _context: SdkContext) -> SdkResult<SdkOutput> {
        let leds = if input.telemetry.flags.red_flag || input.telemetry.flags.yellow_flag {
            // Show flag colors - simplified implementation
            vec![SdkLedColor { r: 255, g: 0, b: 0 }; input.led_count as usize]
        } else {
            // Show RPM pattern - simplified implementation
            let normalized_rpm = (input.telemetry.rpm / self.max_rpm).clamp(0.0, 1.0);
            let active_leds = (normalized_rpm * input.led_count as f32) as u32;
            
            (0..input.led_count)
                .map(|i| {
                    if i < active_leds {
                        if normalized_rpm > self.shift_point {
                            SdkLedColor { r: 255, g: 0, b: 0 } // Red
                        } else {
                            SdkLedColor { r: 0, g: 255, b: 0 } // Green
                        }
                    } else {
                        SdkLedColor { r: 0, g: 0, b: 0 } // Off
                    }
                })
                .collect()
        };
        
        Ok(SdkOutput::Led {
            leds,
            brightness: 1.0,
            duration_ms: 50,
        })
    }
    
    fn shutdown(&mut self) -> SdkResult<()> {
        Ok(())
    }
}