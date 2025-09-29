//! Sample telemetry processing plugin

use racing_wheel_plugins::sdk::*;
use serde_json::Value;

/// Sample telemetry processor that adds custom data
#[derive(Default)]
pub struct SampleTelemetryPlugin {
    config: Value,
    frame_count: u64,
}

impl WasmPlugin for SampleTelemetryPlugin {
    fn initialize(&mut self, config: Value) -> SdkResult<()> {
        self.config = config;
        self.frame_count = 0;
        Ok(())
    }
    
    fn process_telemetry(&mut self, mut input: SdkTelemetry, _context: SdkContext) -> SdkResult<SdkOutput> {
        self.frame_count += 1;
        
        // Add custom data
        input.custom_data.insert(
            "sample_plugin_frame_count".to_string(),
            Value::Number(self.frame_count.into()),
        );
        
        // Slightly modify FFB based on slip ratio
        if input.slip_ratio > 0.1 {
            input.ffb_scalar *= 1.1; // Increase FFB when slipping
        }
        
        Ok(SdkOutput::Telemetry {
            telemetry: input,
            custom_data: Value::Object(serde_json::Map::new()),
        })
    }
    
    fn process_led_mapping(&mut self, _input: SdkLedInput, _context: SdkContext) -> SdkResult<SdkOutput> {
        Err(SdkError::CapabilityRequired("ControlLeds".to_string()))
    }
    
    fn shutdown(&mut self) -> SdkResult<()> {
        Ok(())
    }
}

// Export the plugin for WASM
racing_wheel_plugins::export_wasm_plugin!(SampleTelemetryPlugin);