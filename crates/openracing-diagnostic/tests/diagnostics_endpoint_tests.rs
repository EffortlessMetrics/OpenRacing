//! Deep tests for diagnostics endpoint data structures.
//!
//! Covers: health check response format, metrics endpoint data,
//! performance counters, and diagnostic stream behavior.

use openracing_diagnostic::support_bundle::{
    CpuInfo, DiskInfo, HardwareInfo, MemoryInfo, NetworkInfo, OsInfo, ProcessInfo, SystemInfo,
};
use openracing_diagnostic::{
    DiagnosticError, FrameData, HealthEventData, SafetyStateSimple, StreamA, StreamB, StreamC,
    StreamReader, TelemetryData,
};
use std::collections::HashMap;
use std::time::SystemTime;

type TestResult = Result<(), Box<dyn std::error::Error>>;

fn make_system_info() -> SystemInfo {
    SystemInfo {
        os_info: OsInfo {
            name: "TestOS".to_string(),
            version: "10.0".to_string(),
            kernel_version: "5.15.0".to_string(),
            hostname: "test-host".to_string(),
            uptime_seconds: 86400,
        },
        hardware_info: HardwareInfo {
            cpu_info: CpuInfo {
                brand: "Test CPU x86_64".to_string(),
                frequency_mhz: 3600,
                core_count: 8,
                usage_percent: 12.5,
            },
            memory_info: MemoryInfo {
                total_mb: 32768,
                available_mb: 24576,
                used_mb: 8192,
                usage_percent: 25.0,
            },
            disk_info: vec![DiskInfo {
                name: "sda".to_string(),
                mount_point: "/".to_string(),
                total_gb: 500,
                available_gb: 350,
                usage_percent: 30.0,
            }],
            network_info: vec![NetworkInfo {
                name: "eth0".to_string(),
                bytes_received: 1_000_000,
                bytes_transmitted: 500_000,
                packets_received: 10_000,
                packets_transmitted: 5_000,
            }],
        },
        process_info: ProcessInfo {
            pid: 42,
            memory_usage_mb: 256,
            cpu_usage_percent: 5.0,
            thread_count: 12,
            start_time: SystemTime::now(),
        },
        environment: {
            let mut env = HashMap::new();
            env.insert("CARGO_PKG_NAME".to_string(), "test".to_string());
            env.insert("RUST_LOG".to_string(), "debug".to_string());
            env
        },
        collected_at: SystemTime::now(),
    }
}

fn make_frame(seq: u16) -> FrameData {
    FrameData {
        ffb_in: seq as f32 * 0.1,
        torque_out: seq as f32 * 0.05,
        wheel_speed: seq as f32 * 2.0,
        hands_off: false,
        ts_mono_ns: seq as u64 * 1_000_000,
        seq,
    }
}

fn make_health_event(device_id: &str, event_type: &str) -> HealthEventData {
    HealthEventData {
        timestamp_ns: 1_000_000_000,
        device_id: device_id.to_string(),
        event_type: event_type.to_string(),
        context: serde_json::json!({"source": "endpoint_test"}),
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Health check response format
// ═══════════════════════════════════════════════════════════════════════════

mod health_check {
    use super::*;

    #[test]
    fn test_system_info_serializes_to_json() -> TestResult {
        let info = make_system_info();
        let json = serde_json::to_string(&info)?;
        assert!(!json.is_empty());

        // Verify it's valid JSON by round-tripping to Value
        let parsed: serde_json::Value = serde_json::from_str(&json)?;
        assert!(parsed.is_object());
        Ok(())
    }

    #[test]
    fn test_os_info_fields_present_in_json() -> TestResult {
        let info = make_system_info();
        let json = serde_json::to_string(&info)?;
        let parsed: serde_json::Value = serde_json::from_str(&json)?;

        let os = parsed.get("os_info").ok_or("missing os_info")?;
        assert_eq!(os.get("name").and_then(|v| v.as_str()), Some("TestOS"));
        assert_eq!(os.get("version").and_then(|v| v.as_str()), Some("10.0"));
        assert_eq!(
            os.get("kernel_version").and_then(|v| v.as_str()),
            Some("5.15.0")
        );
        assert_eq!(
            os.get("hostname").and_then(|v| v.as_str()),
            Some("test-host")
        );
        assert_eq!(
            os.get("uptime_seconds").and_then(|v| v.as_u64()),
            Some(86400)
        );
        Ok(())
    }

    #[test]
    fn test_hardware_info_cpu_and_memory_in_json() -> TestResult {
        let info = make_system_info();
        let json = serde_json::to_string(&info)?;
        let parsed: serde_json::Value = serde_json::from_str(&json)?;

        let hw = parsed.get("hardware_info").ok_or("missing hardware_info")?;

        let cpu = hw.get("cpu_info").ok_or("missing cpu_info")?;
        assert_eq!(cpu.get("core_count").and_then(|v| v.as_u64()), Some(8));
        assert_eq!(
            cpu.get("frequency_mhz").and_then(|v| v.as_u64()),
            Some(3600)
        );

        let mem = hw.get("memory_info").ok_or("missing memory_info")?;
        assert_eq!(mem.get("total_mb").and_then(|v| v.as_u64()), Some(32768));
        assert_eq!(
            mem.get("available_mb").and_then(|v| v.as_u64()),
            Some(24576)
        );
        Ok(())
    }

    #[test]
    fn test_process_info_fields_in_json() -> TestResult {
        let info = make_system_info();
        let json = serde_json::to_string(&info)?;
        let parsed: serde_json::Value = serde_json::from_str(&json)?;

        let proc_info = parsed.get("process_info").ok_or("missing process_info")?;
        assert_eq!(proc_info.get("pid").and_then(|v| v.as_u64()), Some(42));
        assert_eq!(
            proc_info.get("memory_usage_mb").and_then(|v| v.as_u64()),
            Some(256)
        );
        assert_eq!(
            proc_info.get("thread_count").and_then(|v| v.as_u64()),
            Some(12)
        );
        Ok(())
    }

    #[test]
    fn test_system_info_json_roundtrip() -> TestResult {
        let info = make_system_info();
        let json = serde_json::to_string_pretty(&info)?;
        let deserialized: SystemInfo = serde_json::from_str(&json)?;

        assert_eq!(deserialized.os_info.name, info.os_info.name);
        assert_eq!(deserialized.os_info.version, info.os_info.version);
        assert_eq!(
            deserialized.hardware_info.cpu_info.core_count,
            info.hardware_info.cpu_info.core_count
        );
        assert_eq!(
            deserialized.hardware_info.memory_info.total_mb,
            info.hardware_info.memory_info.total_mb
        );
        assert_eq!(deserialized.process_info.pid, info.process_info.pid);
        assert_eq!(
            deserialized.environment.get("CARGO_PKG_NAME"),
            Some(&"test".to_string())
        );
        Ok(())
    }

    #[test]
    fn test_disk_and_network_info_in_json() -> TestResult {
        let info = make_system_info();
        let json = serde_json::to_string(&info)?;
        let parsed: serde_json::Value = serde_json::from_str(&json)?;

        let hw = parsed.get("hardware_info").ok_or("missing hardware_info")?;

        let disks = hw
            .get("disk_info")
            .and_then(|v| v.as_array())
            .ok_or("missing disk_info")?;
        assert_eq!(disks.len(), 1);
        assert_eq!(disks[0].get("name").and_then(|v| v.as_str()), Some("sda"));

        let nets = hw
            .get("network_info")
            .and_then(|v| v.as_array())
            .ok_or("missing network_info")?;
        assert_eq!(nets.len(), 1);
        assert_eq!(nets[0].get("name").and_then(|v| v.as_str()), Some("eth0"));
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Metrics endpoint data
// ═══════════════════════════════════════════════════════════════════════════

mod metrics_data {
    use super::*;

    #[test]
    fn test_health_event_serializes_to_json() -> TestResult {
        let event = make_health_event("dev-001", "DeviceConnected");
        let json = serde_json::to_string(&event)?;
        let parsed: serde_json::Value = serde_json::from_str(&json)?;

        assert_eq!(
            parsed.get("device_id").and_then(|v| v.as_str()),
            Some("dev-001")
        );
        assert_eq!(
            parsed.get("event_type").and_then(|v| v.as_str()),
            Some("DeviceConnected")
        );
        assert_eq!(
            parsed.get("timestamp_ns").and_then(|v| v.as_u64()),
            Some(1_000_000_000)
        );
        Ok(())
    }

    #[test]
    fn test_health_event_deserializes_from_json() -> TestResult {
        let json_str = r#"{
            "timestamp_ns": 5000000000,
            "device_id": "moza-r9",
            "event_type": "SafetyFault",
            "context": {"fault": "overcurrent", "severity": "critical"}
        }"#;

        let event: HealthEventData = serde_json::from_str(json_str)?;
        assert_eq!(event.timestamp_ns, 5_000_000_000);
        assert_eq!(event.device_id, "moza-r9");
        assert_eq!(event.event_type, "SafetyFault");
        assert_eq!(
            event.context.get("fault").and_then(|v| v.as_str()),
            Some("overcurrent")
        );
        Ok(())
    }

    #[test]
    fn test_health_event_context_complex_json() -> TestResult {
        let event = HealthEventData {
            timestamp_ns: 0,
            device_id: "dev".to_string(),
            event_type: "Complex".to_string(),
            context: serde_json::json!({
                "nested": {
                    "deep": {
                        "value": 42
                    }
                },
                "array": [1, 2, 3],
                "null_field": null,
                "bool_field": true
            }),
        };

        let json = serde_json::to_string(&event)?;
        let roundtrip: HealthEventData = serde_json::from_str(&json)?;

        assert_eq!(
            roundtrip
                .context
                .get("nested")
                .and_then(|v| v.get("deep"))
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_u64()),
            Some(42)
        );
        assert_eq!(
            roundtrip
                .context
                .get("array")
                .and_then(|v| v.as_array())
                .map(|a| a.len()),
            Some(3)
        );
        assert!(roundtrip.context.get("null_field").is_some());
        Ok(())
    }

    #[test]
    fn test_multiple_health_events_serialize_as_array() -> TestResult {
        let events = vec![
            make_health_event("dev-1", "Connected"),
            make_health_event("dev-1", "Calibrated"),
            make_health_event("dev-2", "Connected"),
        ];

        let json = serde_json::to_string(&events)?;
        let parsed: Vec<serde_json::Value> = serde_json::from_str(&json)?;

        assert_eq!(parsed.len(), 3);
        assert_eq!(
            parsed[0].get("device_id").and_then(|v| v.as_str()),
            Some("dev-1")
        );
        assert_eq!(
            parsed[2].get("device_id").and_then(|v| v.as_str()),
            Some("dev-2")
        );
        Ok(())
    }

    #[test]
    fn test_telemetry_data_serialization() -> TestResult {
        let telemetry = TelemetryData {
            ffb_scalar: 0.85,
            rpm: 7500.0,
            speed_ms: 55.0,
            slip_ratio: 0.03,
            gear: 4,
            car_id: Some("ferrari_488".to_string()),
            track_id: Some("monza".to_string()),
        };

        let json = serde_json::to_string(&telemetry)?;
        let parsed: serde_json::Value = serde_json::from_str(&json)?;

        assert_eq!(parsed.get("gear").and_then(|v| v.as_i64()), Some(4));
        assert_eq!(
            parsed.get("car_id").and_then(|v| v.as_str()),
            Some("ferrari_488")
        );
        assert_eq!(
            parsed.get("track_id").and_then(|v| v.as_str()),
            Some("monza")
        );
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Performance counters
// ═══════════════════════════════════════════════════════════════════════════

mod performance_counters {
    use super::*;

    #[test]
    fn test_stream_a_tracks_frame_count() -> TestResult {
        let mut stream = StreamA::new();
        assert_eq!(stream.record_count(), 0);

        for i in 0..10 {
            stream.record_frame(
                make_frame(i),
                &[0.1, 0.2],
                SafetyStateSimple::SafeTorque,
                100,
            )?;
        }
        assert_eq!(stream.record_count(), 10);
        Ok(())
    }

    #[test]
    fn test_stream_b_tracks_telemetry_count() -> TestResult {
        let mut stream = StreamB::with_rate(1_000_000.0); // very high rate to avoid limiting

        let telemetry = TelemetryData::default();
        let recorded = stream.record_telemetry(telemetry)?;
        assert!(recorded, "first telemetry record should succeed");
        assert_eq!(stream.record_count(), 1);
        Ok(())
    }

    #[test]
    fn test_stream_c_tracks_event_count() -> TestResult {
        let mut stream = StreamC::new();
        assert_eq!(stream.record_count(), 0);

        stream.record_health_event(make_health_event("dev-1", "Connected"))?;
        stream.record_health_event(make_health_event("dev-1", "Calibrated"))?;
        stream.record_health_event(make_health_event("dev-1", "Disconnected"))?;
        assert_eq!(stream.record_count(), 3);
        Ok(())
    }

    #[test]
    fn test_stream_a_reset_clears_state() -> TestResult {
        let mut stream = StreamA::new();
        stream.record_frame(make_frame(0), &[0.1], SafetyStateSimple::SafeTorque, 50)?;
        assert_eq!(stream.record_count(), 1);

        stream.reset();
        assert_eq!(stream.record_count(), 0);
        Ok(())
    }

    #[test]
    fn test_stream_a_get_data_clears_records() -> TestResult {
        let mut stream = StreamA::new();
        stream.record_frame(make_frame(0), &[0.1], SafetyStateSimple::SafeTorque, 50)?;
        stream.record_frame(make_frame(1), &[0.2], SafetyStateSimple::SafeTorque, 60)?;
        assert_eq!(stream.record_count(), 2);

        let data = stream.get_data()?;
        assert!(!data.is_empty());
        assert_eq!(stream.record_count(), 0, "get_data should clear records");
        Ok(())
    }

    #[test]
    fn test_stream_b_rate_limiting_rejects_fast_records() -> TestResult {
        let mut stream = StreamB::with_rate(1.0); // 1 Hz — very slow

        let t1 = TelemetryData::default();
        let t2 = TelemetryData::default();

        let first = stream.record_telemetry(t1)?;
        let second = stream.record_telemetry(t2)?;

        assert!(first, "first record should succeed");
        assert!(!second, "immediate second record should be rate-limited");
        assert_eq!(stream.record_count(), 1);
        Ok(())
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Diagnostic stream
// ═══════════════════════════════════════════════════════════════════════════

mod diagnostic_stream {
    use super::*;

    #[test]
    fn test_stream_a_data_roundtrip() -> TestResult {
        let mut stream = StreamA::new();
        for i in 0..5 {
            stream.record_frame(
                make_frame(i),
                &[i as f32 * 0.01],
                SafetyStateSimple::SafeTorque,
                100 + i as u64,
            )?;
        }

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);
        let mut count = 0u16;

        while let Some(record) = reader.read_stream_a_record()? {
            assert!(
                (record.frame.seq as f32 * 0.1 - record.frame.ffb_in).abs() < 0.01,
                "frame data mismatch at seq {}",
                count
            );
            assert_eq!(record.node_outputs.len(), 1);
            count += 1;
        }
        assert_eq!(count, 5);
        assert!(reader.is_at_end());
        Ok(())
    }

    #[test]
    fn test_stream_b_data_roundtrip() -> TestResult {
        let mut stream = StreamB::with_rate(1_000_000.0);

        let telemetry = TelemetryData {
            ffb_scalar: 0.75,
            rpm: 6000.0,
            speed_ms: 40.0,
            slip_ratio: 0.02,
            gear: 3,
            car_id: Some("test_car".to_string()),
            track_id: None,
        };
        stream.record_telemetry(telemetry)?;

        let data = stream.get_data()?;
        let mut reader = StreamReader::new(data);

        let record = reader
            .read_stream_b_record()?
            .ok_or("expected one record")?;

        assert!((record.telemetry.rpm - 6000.0).abs() < 0.01);
        assert_eq!(record.telemetry.gear, 3);
        assert_eq!(record.telemetry.car_id.as_deref(), Some("test_car"));
        assert!(record.telemetry.track_id.is_none());

        let next = reader.read_stream_b_record()?;
        assert!(next.is_none(), "should have no more records");
        Ok(())
    }

    #[test]
    fn test_stream_c_records_and_produces_data() -> TestResult {
        let mut stream = StreamC::new();
        stream.record_health_event(make_health_event("dev-1", "SafetyFault"))?;
        stream.record_health_event(make_health_event("dev-2", "Connected"))?;
        assert_eq!(stream.record_count(), 2);

        // get_data serialises records to bytes and clears internal buffer
        let data = stream.get_data()?;
        assert!(
            !data.is_empty(),
            "serialized stream C data should be non-empty"
        );
        assert_eq!(stream.record_count(), 0, "get_data should clear records");
        Ok(())
    }

    #[test]
    fn test_stream_reader_empty_data() -> TestResult {
        let reader_data: Vec<u8> = Vec::new();
        let mut reader = StreamReader::new(reader_data);

        let record = reader.read_stream_a_record()?;
        assert!(record.is_none(), "empty data should yield None");
        assert!(reader.is_at_end());
        assert_eq!(reader.position(), 0);
        Ok(())
    }

    #[test]
    fn test_stream_reader_truncated_length_prefix() -> TestResult {
        // Only 2 bytes — not enough for a 4-byte length prefix
        let truncated = vec![0x01, 0x02];
        let mut reader = StreamReader::new(truncated);

        let result = reader.read_stream_a_record();
        assert!(result.is_err(), "truncated data should produce an error");
        if let Err(DiagnosticError::Deserialization(msg)) = result {
            assert!(
                msg.contains("Incomplete"),
                "error should mention incomplete data: {msg}"
            );
        }
        Ok(())
    }

    #[test]
    fn test_stream_reader_reset() -> TestResult {
        let mut stream = StreamA::new();
        stream.record_frame(make_frame(0), &[0.1], SafetyStateSimple::SafeTorque, 50)?;
        let data = stream.get_data()?;

        let mut reader = StreamReader::new(data);
        let _ = reader.read_stream_a_record()?;
        assert!(reader.is_at_end());

        reader.reset();
        assert_eq!(reader.position(), 0);
        assert!(!reader.is_at_end());

        let record = reader.read_stream_a_record()?;
        assert!(record.is_some(), "should read record again after reset");
        Ok(())
    }

    #[test]
    fn test_multiple_streams_operate_independently() -> TestResult {
        let mut stream_a = StreamA::new();
        let mut stream_b = StreamB::with_rate(1_000_000.0);
        let mut stream_c = StreamC::new();

        // Record different counts on each stream
        stream_a.record_frame(make_frame(0), &[], SafetyStateSimple::SafeTorque, 50)?;
        stream_a.record_frame(make_frame(1), &[], SafetyStateSimple::SafeTorque, 60)?;
        stream_b.record_telemetry(TelemetryData::default())?;
        stream_c.record_health_event(make_health_event("dev-1", "A"))?;
        stream_c.record_health_event(make_health_event("dev-1", "B"))?;
        stream_c.record_health_event(make_health_event("dev-1", "C"))?;

        assert_eq!(stream_a.record_count(), 2);
        assert_eq!(stream_b.record_count(), 1);
        assert_eq!(stream_c.record_count(), 3);

        // Serializing one stream doesn't affect others
        let _data_a = stream_a.get_data()?;
        assert_eq!(stream_a.record_count(), 0);
        assert_eq!(stream_b.record_count(), 1);
        assert_eq!(stream_c.record_count(), 3);
        Ok(())
    }

    #[test]
    fn test_frame_data_default_values() -> TestResult {
        let frame = FrameData::default();
        assert!((frame.ffb_in - 0.0).abs() < f32::EPSILON);
        assert!((frame.torque_out - 0.0).abs() < f32::EPSILON);
        assert!((frame.wheel_speed - 0.0).abs() < f32::EPSILON);
        assert!(!frame.hands_off);
        assert_eq!(frame.ts_mono_ns, 0);
        assert_eq!(frame.seq, 0);
        Ok(())
    }

    #[test]
    fn test_safety_state_faulted_variant() -> TestResult {
        let faulted = SafetyStateSimple::Faulted {
            fault_type: "overcurrent".to_string(),
        };

        // Verify it serializes with the fault_type field
        let json = serde_json::to_string(&faulted)?;
        assert!(json.contains("overcurrent"));

        let roundtrip: SafetyStateSimple = serde_json::from_str(&json)?;
        if let SafetyStateSimple::Faulted { fault_type } = roundtrip {
            assert_eq!(fault_type, "overcurrent");
        } else {
            return Err("expected Faulted variant after roundtrip".into());
        }
        Ok(())
    }
}
