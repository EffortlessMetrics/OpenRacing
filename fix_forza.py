f = r'crates\telemetry-adapters\src\forza.rs'
with open(f, 'r', encoding='utf-8') as fp:
    c = fp.read()

orig_len = len(c)

# 1. Add #[allow(dead_code)] before two unused constants
c = c.replace(
    'const OFF_ENGINE_IDLE_RPM: usize = 12; // f32 (unused but documented)',
    '#[allow(dead_code)]\nconst OFF_ENGINE_IDLE_RPM: usize = 12; // f32 (unused but documented)'
)
c = c.replace(
    'const OFF_ACCEL_Y: usize = 24; // f32 \xe2\x80\x93 vertical (up = positive)',
    '#[allow(dead_code)]\nconst OFF_ACCEL_Y: usize = 24; // f32 \xe2\x80\x93 vertical (up = positive)'
)

# 2. Fix make_sled_packet function
old_fn = (
    '    fn make_sled_packet(\n'
    '        is_race_on: i32,\n'
    '        rpm: f32,\n'
    '        throttle: f32,\n'
    '        brake: f32,\n'
    '        gear: f32,\n'
    '        steer: f32,\n'
    '        vel: (f32, f32, f32),\n'
    '    ) -> Vec<u8> {\n'
    '        let mut data = vec![0u8; FORZA_SLED_SIZE];\n'
    '        data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&is_race_on.to_le_bytes());\n'
    '        data[OFF_ENGINE_MAX_RPM..OFF_ENGINE_MAX_RPM + 4].copy_from_slice(&8000.0f32.to_le_bytes());\n'
    '        data[OFF_CURRENT_RPM..OFF_CURRENT_RPM + 4].copy_from_slice(&rpm.to_le_bytes());\n'
    '        data[OFF_ACCEL..OFF_ACCEL + 4].copy_from_slice(&throttle.to_le_bytes());\n'
    '        data[OFF_BRAKE..OFF_BRAKE + 4].copy_from_slice(&brake.to_le_bytes());\n'
    '        data[OFF_GEAR..OFF_GEAR + 4].copy_from_slice(&gear.to_le_bytes());\n'
    '        data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&steer.to_le_bytes());\n'
    '        data[OFF_VEL_X..OFF_VEL_X + 4].copy_from_slice(&vel.0.to_le_bytes());\n'
    '        data[OFF_VEL_Y..OFF_VEL_Y + 4].copy_from_slice(&vel.1.to_le_bytes());\n'
    '        data[OFF_VEL_Z..OFF_VEL_Z + 4].copy_from_slice(&vel.2.to_le_bytes());\n'
    '        data\n'
    '    }'
)
new_fn = (
    '    fn make_sled_packet(is_race_on: i32, rpm: f32, vel: (f32, f32, f32)) -> Vec<u8> {\n'
    '        let mut data = vec![0u8; FORZA_SLED_SIZE];\n'
    '        data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&is_race_on.to_le_bytes());\n'
    '        data[OFF_ENGINE_MAX_RPM..OFF_ENGINE_MAX_RPM + 4].copy_from_slice(&8000.0f32.to_le_bytes());\n'
    '        data[OFF_CURRENT_RPM..OFF_CURRENT_RPM + 4].copy_from_slice(&rpm.to_le_bytes());\n'
    '        data[OFF_VEL_X..OFF_VEL_X + 4].copy_from_slice(&vel.0.to_le_bytes());\n'
    '        data[OFF_VEL_Y..OFF_VEL_Y + 4].copy_from_slice(&vel.1.to_le_bytes());\n'
    '        data[OFF_VEL_Z..OFF_VEL_Z + 4].copy_from_slice(&vel.2.to_le_bytes());\n'
    '        data\n'
    '    }'
)
if old_fn in c:
    c = c.replace(old_fn, new_fn)
    print("OK: make_sled_packet function replaced")
else:
    print("ERROR: make_sled_packet not found")

# 3. Fix call sites: remove the 4 middle args
import re

# Pattern: make_sled_packet(is_race_on, rpm, throttle, brake, gear, steer, (vel))
# Replace with: make_sled_packet(is_race_on, rpm, (vel))
call_pattern = r'make_sled_packet\((\d+), (\d+\.\d+), [\d.]+, -?[\d.]+, [\d.]+, [\d.]+, (\([-\d.,\s]+\))\)'
call_repl = r'make_sled_packet(\1, \2, \3)'
c, n = re.subn(call_pattern, call_repl, c)
print(f"OK: {n} call sites fixed")

# 4. Fix test_parse_sled_valid: remove wrong assertions (throttle, gear, steering_angle)
old_assertions = (
    '        assert!((result.rpm - 5000.0).abs() < 0.01);\n'
    '        assert!((result.throttle - 0.7).abs() < 0.001);\n'
    '        assert!((result.brake).abs() < 0.001);\n'
    '        assert_eq!(result.gear, 3);\n'
    '        assert!((result.steering_angle - 0.25).abs() < 0.001);\n'
    '        assert!((result.speed_ms - 20.0).abs() < 0.01);'
)
new_assertions = (
    '        assert!((result.rpm - 5000.0).abs() < 0.01);\n'
    '        assert!((result.speed_ms - 20.0).abs() < 0.01);'
)
if old_assertions in c:
    c = c.replace(old_assertions, new_assertions)
    print("OK: test_parse_sled_valid assertions fixed")
else:
    print("ERROR: test_parse_sled_valid assertions not found")

# 5. Fix test_parse_sled_gear_reverse: rename and fix assertion
old_gear_test = (
    '    fn test_parse_sled_gear_reverse() -> TestResult {\n'
    '        let data = make_sled_packet(1, 1000.0, (-5.0, 0.0, 0.0));\n'
    '        let result = parse_forza_sled(&data)?;\n'
    '        assert_eq!(result.gear, -1);\n'
    '        Ok(())\n'
    '    }'
)
new_gear_test = (
    '    fn test_parse_sled_reverse_velocity() -> TestResult {\n'
    '        let data = make_sled_packet(1, 1000.0, (-5.0, 0.0, 0.0));\n'
    '        let result = parse_forza_sled(&data)?;\n'
    '        // Speed is magnitude of velocity vector; direction is not tracked in Sled format\n'
    '        assert!((result.speed_ms - 5.0).abs() < 0.01);\n'
    '        Ok(())\n'
    '    }'
)
if old_gear_test in c:
    c = c.replace(old_gear_test, new_gear_test)
    print("OK: test_parse_sled_gear_reverse fixed")
else:
    print("ERROR: test_parse_sled_gear_reverse not found")

# 6. Fix test_normalization_clamp: replace with velocity magnitude test
old_clamp = (
    '    fn test_normalization_clamp() -> TestResult {\n'
    '        let data = make_sled_packet(1, 5000.0, (20.0, 0.0, 0.0));\n'
    '        let result = parse_forza_sled(&data)?;\n'
    '        assert!((result.throttle - 1.0).abs() < 0.001);\n'
    '        assert!((result.brake).abs() < 0.001);\n'
    '        assert!((result.steering_angle - 1.0).abs() < 0.001);\n'
    '        Ok(())\n'
    '    }'
)
new_clamp = (
    '    fn test_sled_speed_is_velocity_magnitude() -> TestResult {\n'
    '        // sqrt(3^2 + 4^2) = 5.0\n'
    '        let data = make_sled_packet(1, 5000.0, (3.0, 4.0, 0.0));\n'
    '        let result = parse_forza_sled(&data)?;\n'
    '        assert!((result.speed_ms - 5.0).abs() < 0.01);\n'
    '        Ok(())\n'
    '    }'
)
if old_clamp in c:
    c = c.replace(old_clamp, new_clamp)
    print("OK: test_normalization_clamp replaced")
else:
    print("ERROR: test_normalization_clamp not found")

# 7. Fix proptest: parse_sled_steering_clamped -> parse_sled_speed_nonneg
old_proptest = (
    '        fn parse_sled_steering_clamped(steer in any::<f32>()) {\n'
    '            let mut data = vec![0u8; FORZA_SLED_SIZE];\n'
    '            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());\n'
    '            data[OFF_STEER..OFF_STEER + 4].copy_from_slice(&steer.to_le_bytes());\n'
    '            if let Ok(result) = parse_forza_sled(&data) {\n'
    '                prop_assert!(result.steering_angle >= -1.0);\n'
    '                prop_assert!(result.steering_angle <= 1.0);\n'
    '            }\n'
    '        }'
)
new_proptest = (
    '        fn parse_sled_speed_nonneg_on_arbitrary_velocity(\n'
    '            vel_x in any::<f32>(),\n'
    '            vel_y in any::<f32>(),\n'
    '            vel_z in any::<f32>(),\n'
    '        ) {\n'
    '            let mut data = vec![0u8; FORZA_SLED_SIZE];\n'
    '            data[OFF_IS_RACE_ON..OFF_IS_RACE_ON + 4].copy_from_slice(&1i32.to_le_bytes());\n'
    '            data[OFF_VEL_X..OFF_VEL_X + 4].copy_from_slice(&vel_x.to_le_bytes());\n'
    '            data[OFF_VEL_Y..OFF_VEL_Y + 4].copy_from_slice(&vel_y.to_le_bytes());\n'
    '            data[OFF_VEL_Z..OFF_VEL_Z + 4].copy_from_slice(&vel_z.to_le_bytes());\n'
    '            if let Ok(result) = parse_forza_sled(&data) {\n'
    '                prop_assert!(result.speed_ms >= 0.0);\n'
    '            }\n'
    '        }'
)
if old_proptest in c:
    c = c.replace(old_proptest, new_proptest)
    print("OK: parse_sled_steering_clamped proptest fixed")
else:
    print("ERROR: parse_sled_steering_clamped proptest not found")

print(f"\nLength change: {orig_len} -> {len(c)}")

with open(f, 'w', encoding='utf-8', newline='') as fp:
    fp.write(c)
print("File written successfully")
