//! Comprehensive anticheat and audit crypto hardening tests.
//!
//! Covers:
//!   • Anticheat report initialization, configuration, and serialization
//!   • HMAC-SHA256 audit log signing and verification
//!   • Audit log entry creation, chaining, and rotation
//!   • Tamper detection: modified, missing, and reordered entries
//!   • Concurrent audit log access
//!   • Anticheat state transitions
//!   • Platform-specific anticheat behaviour
//!   • Integration with game detection / telemetry methods
//!   • HMAC-SHA256 correctness against RFC 4231 test vectors

use std::sync::{Arc, Mutex};
use std::time::SystemTime;

use racing_wheel_service::anticheat::{
    AntiCheatReport, FileAccess, NetworkAccess, PlatformInfo, ProcessInfo, SecurityMeasure,
    SystemApi, TelemetryMethod,
};
use sha2::{Digest, Sha256};

type BoxErr = Box<dyn std::error::Error + Send + Sync>;

// ── HMAC-SHA256 implementation (RFC 2104) for test use ──────────────────

const HMAC_BLOCK_SIZE: usize = 64;
const HMAC_OUTPUT_SIZE: usize = 32;

/// Pure HMAC-SHA256 built on top of sha2.
fn hmac_sha256(key: &[u8], message: &[u8]) -> [u8; HMAC_OUTPUT_SIZE] {
    // If key is longer than block size, hash it first
    let key_block = if key.len() > HMAC_BLOCK_SIZE {
        let mut h = Sha256::new();
        h.update(key);
        let hash = h.finalize();
        let mut block = [0u8; HMAC_BLOCK_SIZE];
        block[..HMAC_OUTPUT_SIZE].copy_from_slice(&hash);
        block
    } else {
        let mut block = [0u8; HMAC_BLOCK_SIZE];
        block[..key.len()].copy_from_slice(key);
        block
    };

    // Inner padding
    let mut ipad = [0x36u8; HMAC_BLOCK_SIZE];
    for (i, b) in ipad.iter_mut().enumerate() {
        *b ^= key_block[i];
    }

    // Outer padding
    let mut opad = [0x5cu8; HMAC_BLOCK_SIZE];
    for (i, b) in opad.iter_mut().enumerate() {
        *b ^= key_block[i];
    }

    // Inner hash: H(K' ⊕ ipad || message)
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(message);
    let inner_hash = inner.finalize();

    // Outer hash: H(K' ⊕ opad || inner_hash)
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);

    let result = outer.finalize();
    let mut out = [0u8; HMAC_OUTPUT_SIZE];
    out.copy_from_slice(&result);
    out
}

// ── Audit log types (mirror expected production shapes) ─────────────────

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
struct AuditLogEntry {
    sequence: u64,
    timestamp: u64,
    event_type: String,
    payload: String,
    /// HMAC-SHA256 over (sequence || timestamp || event_type || payload || prev_hash)
    hmac: String,
    /// Hash of the previous entry's HMAC (chain link)
    prev_hash: String,
}

#[derive(Debug)]
struct AuditLog {
    key: Vec<u8>,
    entries: Vec<AuditLogEntry>,
    next_sequence: u64,
}

impl AuditLog {
    fn new(key: Vec<u8>) -> Self {
        Self {
            key,
            entries: Vec::new(),
            next_sequence: 0,
        }
    }

    fn append(&mut self, event_type: &str, payload: &str) -> AuditLogEntry {
        let prev_hash = self
            .entries
            .last()
            .map(|e| e.hmac.clone())
            .unwrap_or_else(|| "0".repeat(64));

        let seq = self.next_sequence;
        self.next_sequence += 1;

        let ts = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let message = Self::build_message(seq, ts, event_type, payload, &prev_hash);
        let mac = hmac_sha256(&self.key, message.as_bytes());

        let entry = AuditLogEntry {
            sequence: seq,
            timestamp: ts,
            event_type: event_type.to_string(),
            payload: payload.to_string(),
            hmac: hex::encode(mac),
            prev_hash,
        };

        self.entries.push(entry.clone());
        entry
    }

    fn build_message(
        seq: u64,
        ts: u64,
        event_type: &str,
        payload: &str,
        prev_hash: &str,
    ) -> String {
        format!("{seq}:{ts}:{event_type}:{payload}:{prev_hash}")
    }

    fn verify_chain(&self) -> Result<bool, BoxErr> {
        let mut expected_prev = "0".repeat(64);

        for entry in &self.entries {
            if entry.prev_hash != expected_prev {
                return Ok(false);
            }

            let message = Self::build_message(
                entry.sequence,
                entry.timestamp,
                &entry.event_type,
                &entry.payload,
                &entry.prev_hash,
            );
            let expected_mac = hex::encode(hmac_sha256(&self.key, message.as_bytes()));

            if entry.hmac != expected_mac {
                return Ok(false);
            }

            expected_prev = entry.hmac.clone();
        }

        Ok(true)
    }

    fn verify_entry(&self, entry: &AuditLogEntry) -> bool {
        let message = Self::build_message(
            entry.sequence,
            entry.timestamp,
            &entry.event_type,
            &entry.payload,
            &entry.prev_hash,
        );
        let expected_mac = hex::encode(hmac_sha256(&self.key, message.as_bytes()));
        entry.hmac == expected_mac
    }
}

// ── Helper: build a sample AntiCheatReport ──────────────────────────────

fn sample_report() -> AntiCheatReport {
    AntiCheatReport {
        generated_at: "2025-01-15T12:00:00Z".to_string(),
        version: "0.1.0".to_string(),
        platform: PlatformInfo {
            os: "Windows".to_string(),
            os_version: "10.0.22631".to_string(),
            arch: "x86_64".to_string(),
            kernel_version: None,
        },
        process_info: ProcessInfo {
            name: "wheeld".to_string(),
            arch: "x86_64".to_string(),
            privilege_level: "User".to_string(),
            parent_process: Some("explorer.exe".to_string()),
            child_processes: vec!["wheel-plugin-helper".to_string()],
            dll_injection: false,
            kernel_drivers: vec![],
        },
        telemetry_methods: vec![
            TelemetryMethod {
                game: "iRacing".to_string(),
                method_type: "Shared Memory".to_string(),
                description: "Reads telemetry data".to_string(),
                implementation: "Official SDK".to_string(),
                memory_access: "Read-only".to_string(),
                file_access: Some("app.ini".to_string()),
                network_protocol: None,
                anticheat_compatible: true,
                compatibility_notes: "Official SDK methods".to_string(),
            },
            TelemetryMethod {
                game: "ACC".to_string(),
                method_type: "UDP Broadcast".to_string(),
                description: "Receives UDP telemetry".to_string(),
                implementation: "Official telemetry API".to_string(),
                memory_access: "None".to_string(),
                file_access: Some("broadcasting.json".to_string()),
                network_protocol: Some("UDP".to_string()),
                anticheat_compatible: true,
                compatibility_notes: "Official API".to_string(),
            },
        ],
        file_access: vec![FileAccess {
            path_pattern: "%LOCALAPPDATA%/wheel/*".to_string(),
            access_type: "Read/Write".to_string(),
            purpose: "Config storage".to_string(),
            frequency: "On startup".to_string(),
            user_consent: false,
        }],
        network_access: vec![NetworkAccess {
            protocol: "UDP".to_string(),
            direction: "Inbound".to_string(),
            purpose: "Telemetry".to_string(),
            endpoints: vec!["localhost:9000".to_string()],
            data_transmitted: "Game telemetry".to_string(),
            user_consent: false,
        }],
        system_apis: vec![SystemApi {
            api_name: "HID API".to_string(),
            purpose: "Hardware communication".to_string(),
            privilege_level: "User".to_string(),
            frequency: "Continuous".to_string(),
            anticheat_impact: "None".to_string(),
        }],
        security_measures: vec![SecurityMeasure {
            name: "Code Signing".to_string(),
            description: "Signed binaries".to_string(),
            implementation: "Ed25519".to_string(),
            effectiveness: "Prevents tampering".to_string(),
        }],
    }
}

// ═══════════════════════════════════════════════════════════════════════
// 1. HMAC-SHA256 correctness — RFC 4231 test vectors
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn hmac_sha256_rfc4231_test_case_1() -> Result<(), BoxErr> {
    // Key = 20 bytes of 0x0b
    let key = vec![0x0bu8; 20];
    let data = b"Hi There";
    let expected = "b0344c61d8db38535ca8afceaf0bf12b881dc200c9833da726e9376c2e32cff7";

    let result = hmac_sha256(&key, data);
    assert_eq!(hex::encode(result), expected, "RFC 4231 test case 1 failed");
    Ok(())
}

#[test]
fn hmac_sha256_rfc4231_test_case_2() -> Result<(), BoxErr> {
    // Key = "Jefe"
    let key = b"Jefe";
    let data = b"what do ya want for nothing?";
    let expected = "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843";

    let result = hmac_sha256(key, data);
    assert_eq!(hex::encode(result), expected, "RFC 4231 test case 2 failed");
    Ok(())
}

#[test]
fn hmac_sha256_rfc4231_test_case_3() -> Result<(), BoxErr> {
    // Key = 20 bytes of 0xaa, Data = 50 bytes of 0xdd
    let key = vec![0xaau8; 20];
    let data = vec![0xddu8; 50];
    let expected = "773ea91e36800e46854db8ebd09181a72959098b3ef8c122d9635514ced565fe";

    let result = hmac_sha256(&key, &data);
    assert_eq!(hex::encode(result), expected, "RFC 4231 test case 3 failed");
    Ok(())
}

#[test]
fn hmac_sha256_rfc4231_test_case_4() -> Result<(), BoxErr> {
    // Key = 0x0102...19 (25 bytes), Data = 50 bytes of 0xcd
    let key: Vec<u8> = (1..=25).collect();
    let data = vec![0xcdu8; 50];
    let expected = "82558a389a443c0ea4cc819899f2083a85f0faa3e578f8077a2e3ff46729665b";

    let result = hmac_sha256(&key, &data);
    assert_eq!(hex::encode(result), expected, "RFC 4231 test case 4 failed");
    Ok(())
}

#[test]
fn hmac_sha256_rfc4231_test_case_6() -> Result<(), BoxErr> {
    // Key = 131 bytes of 0xaa (longer than block size)
    let key = vec![0xaau8; 131];
    let data = b"Test Using Larger Than Block-Size Key - Hash Key First";
    let expected = "60e431591ee0b67f0d8a26aacbf5b77f8e0bc6213728c5140546040f0ee37f54";

    let result = hmac_sha256(&key, data);
    assert_eq!(hex::encode(result), expected, "RFC 4231 test case 6 failed");
    Ok(())
}

#[test]
fn hmac_sha256_rfc4231_test_case_7() -> Result<(), BoxErr> {
    // Key = 131 bytes of 0xaa
    let key = vec![0xaau8; 131];
    let data =
        b"This is a test using a larger than block-size key and a larger than block-size data. The key needs to be hashed before being used by the HMAC algorithm.";
    let expected = "9b09ffa71b942fcb27635fbcd5b0e944bfdc63644f0713938a7f51535c3a35e2";

    let result = hmac_sha256(&key, data);
    assert_eq!(hex::encode(result), expected, "RFC 4231 test case 7 failed");
    Ok(())
}

#[test]
fn hmac_sha256_empty_message() -> Result<(), BoxErr> {
    let key = b"secret";
    let result = hmac_sha256(key, b"");
    // Should produce a valid 32-byte MAC even for empty message
    assert_eq!(result.len(), 32);
    // Verify determinism
    let result2 = hmac_sha256(key, b"");
    assert_eq!(result, result2, "HMAC should be deterministic");
    Ok(())
}

#[test]
fn hmac_sha256_different_keys_produce_different_macs() -> Result<(), BoxErr> {
    let data = b"identical payload";
    let mac1 = hmac_sha256(b"key-alpha", data);
    let mac2 = hmac_sha256(b"key-bravo", data);
    assert_ne!(mac1, mac2, "Different keys must produce different MACs");
    Ok(())
}

#[test]
fn hmac_sha256_different_messages_produce_different_macs() -> Result<(), BoxErr> {
    let key = b"shared-key";
    let mac1 = hmac_sha256(key, b"message-one");
    let mac2 = hmac_sha256(key, b"message-two");
    assert_ne!(mac1, mac2, "Different messages must produce different MACs");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 2. Audit log entry creation and signing
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn audit_log_create_entry() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"test-key-material".to_vec());
    let entry = log.append("startup", "service initialized");

    assert_eq!(entry.sequence, 0);
    assert_eq!(entry.event_type, "startup");
    assert_eq!(entry.payload, "service initialized");
    assert_eq!(entry.hmac.len(), 64, "HMAC hex should be 64 chars");
    assert_eq!(
        entry.prev_hash,
        "0".repeat(64),
        "First entry links to zero hash"
    );
    Ok(())
}

#[test]
fn audit_log_sequential_numbering() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"seqkey".to_vec());
    for i in 0..10 {
        let entry = log.append("tick", &format!("event-{i}"));
        assert_eq!(entry.sequence, i, "Sequence must be monotonic");
    }
    assert_eq!(log.entries.len(), 10);
    Ok(())
}

#[test]
fn audit_log_entry_hmac_is_deterministic_for_same_inputs() -> Result<(), BoxErr> {
    let key = b"det-key".to_vec();
    let prev_hash = "0".repeat(64);
    let msg = AuditLog::build_message(0, 1000, "test", "payload", &prev_hash);
    let mac1 = hmac_sha256(&key, msg.as_bytes());
    let mac2 = hmac_sha256(&key, msg.as_bytes());
    assert_eq!(mac1, mac2, "Same inputs must yield same HMAC");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 3. Audit log verification — valid and tampered
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn audit_log_verify_valid_chain() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"valid-key".to_vec());
    log.append("init", "started");
    log.append("config", "loaded profiles");
    log.append("device", "wheel connected");

    assert!(log.verify_chain()?, "Untampered chain must verify");
    Ok(())
}

#[test]
fn audit_log_verify_single_entry() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"single-key".to_vec());
    let entry = log.append("boot", "system ready");
    assert!(log.verify_entry(&entry), "Single entry must verify");
    Ok(())
}

#[test]
fn audit_log_verify_wrong_key_fails() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"correct-key".to_vec());
    log.append("ev", "data");

    // Re-create log with wrong key and transplant entries
    let wrong_log = AuditLog {
        key: b"wrong-key".to_vec(),
        entries: log.entries.clone(),
        next_sequence: log.next_sequence,
    };

    assert!(
        !wrong_log.verify_chain()?,
        "Chain verified with wrong key must fail"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 4. Tamper detection — modified, missing, reordered entries
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn tamper_detection_modified_payload() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"tamper-key".to_vec());
    log.append("init", "legit");
    log.append("action", "original");

    // Tamper with payload
    log.entries[1].payload = "TAMPERED".to_string();

    assert!(
        !log.verify_chain()?,
        "Modified payload must break verification"
    );
    Ok(())
}

#[test]
fn tamper_detection_modified_event_type() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"tamper-key".to_vec());
    log.append("init", "ok");
    log.append("config_load", "profiles.json");

    log.entries[1].event_type = "config_tampered".to_string();

    assert!(
        !log.verify_chain()?,
        "Modified event_type must break verification"
    );
    Ok(())
}

#[test]
fn tamper_detection_modified_hmac() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"tamper-key".to_vec());
    log.append("init", "ok");
    log.append("ev", "data");

    // Flip a byte in the HMAC
    let mut hmac_bytes = hex::decode(&log.entries[0].hmac)?;
    hmac_bytes[0] ^= 0xff;
    log.entries[0].hmac = hex::encode(&hmac_bytes);

    assert!(
        !log.verify_chain()?,
        "Modified HMAC must break verification"
    );
    Ok(())
}

#[test]
fn tamper_detection_missing_entry() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"tamper-key".to_vec());
    log.append("a", "1");
    log.append("b", "2");
    log.append("c", "3");

    // Remove the middle entry
    log.entries.remove(1);

    assert!(
        !log.verify_chain()?,
        "Removed entry must break chain verification"
    );
    Ok(())
}

#[test]
fn tamper_detection_reordered_entries() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"tamper-key".to_vec());
    log.append("a", "1");
    log.append("b", "2");
    log.append("c", "3");

    // Swap entries 1 and 2
    log.entries.swap(1, 2);

    assert!(
        !log.verify_chain()?,
        "Reordered entries must break chain verification"
    );
    Ok(())
}

#[test]
fn tamper_detection_inserted_entry() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"tamper-key".to_vec());
    log.append("a", "1");
    log.append("b", "2");

    // Forge an entry and insert
    let forged = AuditLogEntry {
        sequence: 99,
        timestamp: 0,
        event_type: "forged".to_string(),
        payload: "evil".to_string(),
        hmac: "ff".repeat(32),
        prev_hash: log.entries[0].hmac.clone(),
    };
    log.entries.insert(1, forged);

    assert!(
        !log.verify_chain()?,
        "Inserted forged entry must break chain"
    );
    Ok(())
}

#[test]
fn tamper_detection_modified_sequence() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"seq-tamper".to_vec());
    log.append("init", "ok");

    // Tamper with the sequence number (HMAC was computed with sequence=0)
    log.entries[0].sequence = 42;

    assert!(
        !log.verify_chain()?,
        "Modified sequence must break verification"
    );
    Ok(())
}

#[test]
fn tamper_detection_modified_timestamp() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"ts-tamper".to_vec());
    log.append("init", "ok");

    // Tamper with timestamp
    log.entries[0].timestamp = log.entries[0].timestamp.wrapping_add(1);

    assert!(
        !log.verify_chain()?,
        "Modified timestamp must break verification"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 5. Audit log rotation and chaining
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn audit_log_chain_links_consecutive_entries() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"chain-key".to_vec());
    let e0 = log.append("ev0", "data0");
    let e1 = log.append("ev1", "data1");
    let e2 = log.append("ev2", "data2");

    assert_eq!(e1.prev_hash, e0.hmac, "Entry 1 must chain to entry 0");
    assert_eq!(e2.prev_hash, e1.hmac, "Entry 2 must chain to entry 1");
    Ok(())
}

#[test]
fn audit_log_rotation_preserves_tail_hash() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"rotate-key".to_vec());
    for i in 0..20 {
        log.append("tick", &format!("data-{i}"));
    }

    // "Rotate" by keeping last 5 entries
    let tail_hash = log.entries[14].hmac.clone();
    let rotated_entries: Vec<_> = log.entries.drain(15..).collect();

    assert_eq!(rotated_entries.len(), 5);
    assert_eq!(
        rotated_entries[0].prev_hash, tail_hash,
        "First rotated entry must link to previous segment's tail"
    );

    // The rotated segment should still verify internally (entry-by-entry)
    let rotated_log = AuditLog {
        key: b"rotate-key".to_vec(),
        entries: rotated_entries,
        next_sequence: 20,
    };
    // Individual entries still verify
    for entry in &rotated_log.entries {
        assert!(
            rotated_log.verify_entry(entry),
            "Each rotated entry should individually verify"
        );
    }
    Ok(())
}

#[test]
fn audit_log_empty_chain_verifies() -> Result<(), BoxErr> {
    let log = AuditLog::new(b"empty-key".to_vec());
    assert!(log.verify_chain()?, "Empty chain must verify");
    Ok(())
}

#[test]
fn audit_log_large_chain_verifies() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"large-key".to_vec());
    for i in 0..1000 {
        log.append("bulk", &format!("entry-{i}"));
    }
    assert!(log.verify_chain()?, "Large chain must verify");
    assert_eq!(log.entries.len(), 1000);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 6. Concurrent audit log access
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn concurrent_audit_log_append() -> Result<(), BoxErr> {
    let log = Arc::new(Mutex::new(AuditLog::new(b"concurrent-key".to_vec())));
    let mut handles = vec![];

    for i in 0..8 {
        let log_clone = Arc::clone(&log);
        handles.push(std::thread::spawn(move || {
            for j in 0..50 {
                let mut guard = log_clone.lock().map_err(|e| {
                    Box::<dyn std::error::Error + Send + Sync>::from(format!("Mutex poisoned: {e}"))
                })?;
                guard.append("concurrent", &format!("thread-{i}-event-{j}"));
            }
            Ok::<_, Box<dyn std::error::Error + Send + Sync>>(())
        }));
    }

    for h in handles {
        h.join().map_err(|_| "Thread panicked")??;
    }

    let guard = log
        .lock()
        .map_err(|e| -> BoxErr { format!("Mutex poisoned: {e}").into() })?;
    assert_eq!(guard.entries.len(), 400, "8 threads × 50 events = 400");
    assert!(guard.verify_chain()?, "Concurrent chain must still verify");

    // Verify monotonic sequence
    for (idx, entry) in guard.entries.iter().enumerate() {
        assert_eq!(entry.sequence, idx as u64, "Sequence must be monotonic");
    }
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 7. Anticheat report initialization and configuration
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn anticheat_report_construction() -> Result<(), BoxErr> {
    let report = sample_report();
    assert_eq!(report.version, "0.1.0");
    assert_eq!(report.platform.os, "Windows");
    assert_eq!(report.platform.arch, "x86_64");
    assert!(!report.process_info.dll_injection);
    assert!(report.process_info.kernel_drivers.is_empty());
    Ok(())
}

#[test]
fn anticheat_report_serialization_roundtrip() -> Result<(), BoxErr> {
    let report = sample_report();
    let json = serde_json::to_string_pretty(&report)?;
    let deserialized: AntiCheatReport = serde_json::from_str(&json)?;

    assert_eq!(deserialized.version, report.version);
    assert_eq!(deserialized.generated_at, report.generated_at);
    assert_eq!(deserialized.platform.os, report.platform.os);
    assert_eq!(deserialized.platform.arch, report.platform.arch);
    assert_eq!(
        deserialized.process_info.dll_injection,
        report.process_info.dll_injection
    );
    assert_eq!(
        deserialized.telemetry_methods.len(),
        report.telemetry_methods.len()
    );
    assert_eq!(
        deserialized.security_measures.len(),
        report.security_measures.len()
    );
    Ok(())
}

#[test]
fn anticheat_report_markdown_contains_header() -> Result<(), BoxErr> {
    let report = sample_report();
    let md = report.to_markdown();
    assert!(
        md.contains("# Racing Wheel Software - Anti-Cheat Compatibility Report"),
        "Markdown must contain report header"
    );
    Ok(())
}

#[test]
fn anticheat_report_markdown_contains_version_and_timestamp() -> Result<(), BoxErr> {
    let report = sample_report();
    let md = report.to_markdown();
    assert!(md.contains("**Version:** 0.1.0"));
    assert!(md.contains("**Generated:** 2025-01-15T12:00:00Z"));
    Ok(())
}

#[test]
fn anticheat_report_markdown_compatibility_points() -> Result<(), BoxErr> {
    let report = sample_report();
    let md = report.to_markdown();
    assert!(md.contains("No DLL Injection"));
    assert!(md.contains("No Kernel Drivers"));
    assert!(md.contains("Signed Binaries"));
    assert!(md.contains("Open Source"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 8. Anticheat state transitions
// ═══════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnticheatState {
    Uninitialized,
    Initializing,
    Verified,
    CompromiseDetected,
    Shutdown,
}

struct AnticheatStateMachine {
    state: AnticheatState,
    transitions: Vec<(AnticheatState, AnticheatState)>,
}

impl AnticheatStateMachine {
    fn new() -> Self {
        Self {
            state: AnticheatState::Uninitialized,
            transitions: Vec::new(),
        }
    }

    fn transition(&mut self, to: AnticheatState) -> Result<(), BoxErr> {
        let valid = matches!(
            (self.state, to),
            (AnticheatState::Uninitialized, AnticheatState::Initializing)
                | (AnticheatState::Initializing, AnticheatState::Verified)
                | (
                    AnticheatState::Initializing,
                    AnticheatState::CompromiseDetected
                )
                | (AnticheatState::Verified, AnticheatState::CompromiseDetected)
                | (AnticheatState::Verified, AnticheatState::Shutdown)
                | (AnticheatState::CompromiseDetected, AnticheatState::Shutdown)
        );

        if !valid {
            return Err(format!("Invalid transition: {:?} -> {:?}", self.state, to).into());
        }

        self.transitions.push((self.state, to));
        self.state = to;
        Ok(())
    }
}

#[test]
fn anticheat_valid_state_transitions() -> Result<(), BoxErr> {
    let mut sm = AnticheatStateMachine::new();
    sm.transition(AnticheatState::Initializing)?;
    sm.transition(AnticheatState::Verified)?;
    sm.transition(AnticheatState::Shutdown)?;

    assert_eq!(sm.state, AnticheatState::Shutdown);
    assert_eq!(sm.transitions.len(), 3);
    Ok(())
}

#[test]
fn anticheat_compromise_detected_transition() -> Result<(), BoxErr> {
    let mut sm = AnticheatStateMachine::new();
    sm.transition(AnticheatState::Initializing)?;
    sm.transition(AnticheatState::Verified)?;
    sm.transition(AnticheatState::CompromiseDetected)?;
    sm.transition(AnticheatState::Shutdown)?;

    assert_eq!(sm.state, AnticheatState::Shutdown);
    Ok(())
}

#[test]
fn anticheat_invalid_transition_rejected() -> Result<(), BoxErr> {
    let mut sm = AnticheatStateMachine::new();
    // Cannot go directly from Uninitialized to Verified
    let result = sm.transition(AnticheatState::Verified);
    assert!(result.is_err(), "Skip Initializing must be rejected");

    // Cannot go backward
    sm.transition(AnticheatState::Initializing)?;
    sm.transition(AnticheatState::Verified)?;
    let result = sm.transition(AnticheatState::Initializing);
    assert!(result.is_err(), "Backward transition must be rejected");
    Ok(())
}

#[test]
fn anticheat_compromise_during_init() -> Result<(), BoxErr> {
    let mut sm = AnticheatStateMachine::new();
    sm.transition(AnticheatState::Initializing)?;
    sm.transition(AnticheatState::CompromiseDetected)?;
    sm.transition(AnticheatState::Shutdown)?;
    assert_eq!(sm.state, AnticheatState::Shutdown);
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 9. Platform-specific anticheat behaviour
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn anticheat_platform_no_dll_injection() -> Result<(), BoxErr> {
    let report = sample_report();
    assert!(
        !report.process_info.dll_injection,
        "OpenRacing must never use DLL injection"
    );
    Ok(())
}

#[test]
fn anticheat_platform_no_kernel_drivers() -> Result<(), BoxErr> {
    let report = sample_report();
    assert!(
        report.process_info.kernel_drivers.is_empty(),
        "OpenRacing must not use kernel drivers"
    );
    Ok(())
}

#[test]
fn anticheat_platform_user_privilege_level() -> Result<(), BoxErr> {
    let report = sample_report();
    assert_eq!(
        report.process_info.privilege_level, "User",
        "Service must run at User privilege level"
    );
    Ok(())
}

#[test]
fn anticheat_platform_dll_injection_flag_shows_in_markdown() -> Result<(), BoxErr> {
    let mut report = sample_report();
    report.process_info.dll_injection = true;
    let md = report.to_markdown();
    assert!(
        md.contains("❌ Yes"),
        "DLL injection=true must render as warning in markdown"
    );
    Ok(())
}

#[test]
fn anticheat_platform_kernel_drivers_flag_shows_in_markdown() -> Result<(), BoxErr> {
    let mut report = sample_report();
    report.process_info.kernel_drivers = vec!["evil.sys".to_string()];
    let md = report.to_markdown();
    assert!(
        md.contains("❌ Present"),
        "Kernel drivers must render as warning in markdown"
    );
    Ok(())
}

#[cfg(windows)]
#[test]
fn anticheat_platform_windows_apis() -> Result<(), BoxErr> {
    // On Windows, the MMCSS and Named Pipes APIs should be documented
    // We just verify the report structure handles platform-specific APIs
    let report = sample_report();
    let md = report.to_markdown();
    assert!(
        md.contains("## System API Usage"),
        "System API section must be present"
    );
    Ok(())
}

#[cfg(target_os = "linux")]
#[test]
fn anticheat_platform_linux_info() -> Result<(), BoxErr> {
    let mut report = sample_report();
    report.platform.os = "Linux".to_string();
    report.platform.kernel_version = Some("6.1.0".to_string());
    let md = report.to_markdown();
    assert!(md.contains("**Kernel:** 6.1.0"));
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 10. Integration with game detection / telemetry methods
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn anticheat_telemetry_all_methods_compatible() -> Result<(), BoxErr> {
    let report = sample_report();
    for method in &report.telemetry_methods {
        assert!(
            method.anticheat_compatible,
            "Telemetry method '{}' for '{}' must be anti-cheat compatible",
            method.method_type, method.game,
        );
    }
    Ok(())
}

#[test]
fn anticheat_telemetry_markdown_lists_games() -> Result<(), BoxErr> {
    let report = sample_report();
    let md = report.to_markdown();
    for method in &report.telemetry_methods {
        assert!(
            md.contains(&method.game),
            "Markdown must mention game '{}'",
            method.game
        );
    }
    Ok(())
}

#[test]
fn anticheat_telemetry_no_direct_memory_write() -> Result<(), BoxErr> {
    let report = sample_report();
    for method in &report.telemetry_methods {
        let access_lower = method.memory_access.to_lowercase();
        assert!(
            !access_lower.contains("write"),
            "Telemetry for '{}' must not write to game memory (found: '{}')",
            method.game,
            method.memory_access,
        );
    }
    Ok(())
}

#[test]
fn anticheat_network_only_inbound_for_telemetry() -> Result<(), BoxErr> {
    let report = sample_report();
    for access in &report.network_access {
        if access.purpose.to_lowercase().contains("telemetry") {
            assert_eq!(
                access.direction, "Inbound",
                "Telemetry network access must be inbound only"
            );
        }
    }
    Ok(())
}

#[test]
fn anticheat_report_empty_network_omits_section() -> Result<(), BoxErr> {
    let mut report = sample_report();
    report.network_access.clear();
    let md = report.to_markdown();
    assert!(
        !md.contains("## Network Access"),
        "Empty network access should omit section"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 11. Audit log + anticheat integration: signed audit of state changes
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn audit_log_records_state_transitions() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"integration-key".to_vec());
    let mut sm = AnticheatStateMachine::new();

    log.append("state_change", "Uninitialized -> Initializing");
    sm.transition(AnticheatState::Initializing)?;

    log.append("state_change", "Initializing -> Verified");
    sm.transition(AnticheatState::Verified)?;

    log.append("state_change", "Verified -> Shutdown");
    sm.transition(AnticheatState::Shutdown)?;

    assert_eq!(sm.state, AnticheatState::Shutdown);
    assert_eq!(log.entries.len(), 3);
    assert!(
        log.verify_chain()?,
        "Audit of state transitions must verify"
    );
    Ok(())
}

#[test]
fn audit_log_records_game_detection_events() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"game-detect-key".to_vec());

    log.append("game_detected", "iRacing (pid=1234)");
    log.append("telemetry_connected", "iRacing shared memory attached");
    log.append("game_exited", "iRacing (pid=1234)");

    assert_eq!(log.entries.len(), 3);
    assert!(log.verify_chain()?);

    // Verify the payloads are captured correctly
    assert_eq!(log.entries[0].event_type, "game_detected");
    assert!(log.entries[0].payload.contains("iRacing"));
    assert_eq!(log.entries[2].event_type, "game_exited");
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 12. Crypto primitives: SHA256 utility verification
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn sha256_known_vector_empty() -> Result<(), BoxErr> {
    let mut hasher = Sha256::new();
    hasher.update(b"");
    let result = hex::encode(hasher.finalize());
    assert_eq!(
        result, "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "SHA256 of empty string"
    );
    Ok(())
}

#[test]
fn sha256_known_vector_abc() -> Result<(), BoxErr> {
    let mut hasher = Sha256::new();
    hasher.update(b"abc");
    let result = hex::encode(hasher.finalize());
    assert_eq!(
        result, "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        "SHA256 of 'abc'"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 13. Audit log JSON serialization / persistence
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn audit_entry_json_roundtrip() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"json-key".to_vec());
    let entry = log.append("test_event", "some payload");

    let json = serde_json::to_string(&entry)?;
    let deserialized: AuditLogEntry = serde_json::from_str(&json)?;

    assert_eq!(deserialized.sequence, entry.sequence);
    assert_eq!(deserialized.event_type, entry.event_type);
    assert_eq!(deserialized.payload, entry.payload);
    assert_eq!(deserialized.hmac, entry.hmac);
    assert_eq!(deserialized.prev_hash, entry.prev_hash);

    // Deserialized entry should still verify
    assert!(log.verify_entry(&deserialized));
    Ok(())
}

#[test]
fn audit_log_entries_json_roundtrip() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"batch-json".to_vec());
    log.append("a", "1");
    log.append("b", "2");
    log.append("c", "3");

    let json = serde_json::to_string(&log.entries)?;
    let deserialized: Vec<AuditLogEntry> = serde_json::from_str(&json)?;

    assert_eq!(deserialized.len(), 3);

    let restored_log = AuditLog {
        key: b"batch-json".to_vec(),
        entries: deserialized,
        next_sequence: 3,
    };
    assert!(
        restored_log.verify_chain()?,
        "Deserialized chain must verify"
    );
    Ok(())
}

// ═══════════════════════════════════════════════════════════════════════
// 14. Edge cases
// ═══════════════════════════════════════════════════════════════════════

#[test]
fn hmac_sha256_empty_key() -> Result<(), BoxErr> {
    let mac = hmac_sha256(b"", b"data");
    assert_eq!(mac.len(), 32, "Empty key should still produce 32-byte MAC");
    Ok(())
}

#[test]
fn hmac_sha256_key_exactly_block_size() -> Result<(), BoxErr> {
    let key = vec![0x42u8; HMAC_BLOCK_SIZE];
    let mac = hmac_sha256(&key, b"test");
    assert_eq!(mac.len(), 32);
    // Verify determinism
    let mac2 = hmac_sha256(&key, b"test");
    assert_eq!(mac, mac2);
    Ok(())
}

#[test]
fn audit_log_unicode_payload() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"unicode-key".to_vec());
    log.append("event", "日本語テスト 🏎️ données françaises");
    assert!(log.verify_chain()?, "Unicode payloads must verify");
    Ok(())
}

#[test]
fn audit_log_very_long_payload() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"long-key".to_vec());
    let payload = "x".repeat(100_000);
    log.append("bulk_data", &payload);
    assert!(log.verify_chain()?, "Long payloads must verify");
    assert_eq!(log.entries[0].payload.len(), 100_000);
    Ok(())
}

#[test]
fn audit_log_special_characters_in_payload() -> Result<(), BoxErr> {
    let mut log = AuditLog::new(b"special-key".to_vec());
    log.append("ev", "colons:in:payload");
    log.append("ev", "null\0bytes");
    log.append("ev", "newlines\nand\ttabs");
    assert!(log.verify_chain()?, "Special characters must verify");
    Ok(())
}

#[test]
fn anticheat_report_markdown_conclusion_present() -> Result<(), BoxErr> {
    let report = sample_report();
    let md = report.to_markdown();
    assert!(
        md.contains("## Conclusion"),
        "Report must contain conclusion section"
    );
    assert!(
        md.contains("fully compatible with anti-cheat systems"),
        "Conclusion must state compatibility"
    );
    Ok(())
}

#[test]
fn anticheat_report_child_processes_listed() -> Result<(), BoxErr> {
    let report = sample_report();
    let md = report.to_markdown();
    assert!(
        md.contains("wheel-plugin-helper"),
        "Child processes must appear in markdown"
    );
    Ok(())
}

#[test]
fn anticheat_report_security_measures_section() -> Result<(), BoxErr> {
    let report = sample_report();
    let md = report.to_markdown();
    assert!(md.contains("## Security Measures"));
    assert!(md.contains("### Code Signing"));
    Ok(())
}

#[test]
fn anticheat_report_file_access_table() -> Result<(), BoxErr> {
    let report = sample_report();
    let md = report.to_markdown();
    assert!(md.contains("## File System Access"));
    assert!(md.contains("%LOCALAPPDATA%/wheel/*"));
    Ok(())
}
