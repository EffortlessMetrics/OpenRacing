## Summary

Adds 89 comprehensive tests for the `crates/compat/` crate in a new file `crates/compat/tests/compat_layer_tests.rs`.

## Test Coverage

### Migration Path Correctness (9 tests)
- Legacy (v0) to v1 roundtrips preserving FFB gain, DOR, torque cap
- v1 identity migration (pass-through unchanged)
- Full roundtrip: migrate, parse, to_json, parse

### Legacy API Compatibility Shims (8 tests)
- All 5 TelemetryCompat trait methods
- Dynamic dispatch via dyn and Box trait objects
- hands_on field isolation

### Version Detection & Negotiation (13 tests)
- SchemaVersion parsing, ordering, is_current / is_older_than
- BackwardCompatibleParser compatibility checks
- needs_migration for legacy and current formats

### Error Handling (14 tests)
- Invalid schema prefix, missing/non-numeric version, empty string
- Empty/plain-text/array/null/truncated JSON migration failures
- Parser rejection of legacy format without explicit migration

### Edge Cases (10 tests)
- Zero and extreme profile values, extra unknown fields
- Optional sections (leds, haptics, parent, signature)
- Whitespace-heavy JSON, migration idempotency, boundary u8 values

### CompatibleProfile API (6 tests)
- All accessor methods (ffb_gain, dor_deg, torque_cap_nm, game, has_parent)
- to_json roundtrip fidelity, optional section inclusion/omission

### ProfileMigrationService (8 tests)
- Version detection, needs_migration, migrate_with_backup
- MigrationOutcome API: was_migrated, migration_count
- Multiple independent managers produce identical results

### parse_or_migrate (5 tests)
- Direct v1 parsing and automatic legacy migration
- Schema version correctness after auto-migration

### MigrationConfig (3 tests)
- Validation enabled/disabled, backups disabled

### Proptest Fuzzing (10 properties x 100 cases)
- FFB gain, DOR, torque preservation through migration
- v1 migration identity, double-migration idempotency
- Telemetry angle/speed conversion roundtrips
- SchemaVersion ordering consistency

## Rules Compliance
- No unwrap/expect - all tests return Result
- Uses proptest for property-based fuzzing
- Approximate f64 comparison for JSON roundtrip tolerance
