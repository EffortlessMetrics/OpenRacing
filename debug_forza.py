"""
Fix forza.rs: Remove throttle/brake/gear/steer from make_sled_packet test helper
and fix all related test code.
"""
import sys
import os

f = os.path.join(os.getcwd(), r'crates\telemetry-adapters\src\forza.rs')
print(f"Reading: {f}")

with open(f, 'rb') as fp:
    raw = fp.read()

print(f"File size: {len(raw)} bytes")

# Convert to string, preserving exact bytes
content = raw.decode('utf-8')

# Verify old content is there
if 'OFF_ACCEL..OFF_ACCEL' in content:
    print("CONFIRMED: Old content with OFF_ACCEL found")
else:
    print("ERROR: Expected old content not found!")
    sys.exit(1)

lines = content.split('\n')
print(f"Lines: {len(lines)}")

# Show lines around make_sled_packet
for i, line in enumerate(lines):
    if 'fn make_sled_packet' in line or 'OFF_ACCEL' in line or 'OFF_BRAKE' in line or 'OFF_GEAR' in line or 'OFF_STEER' in line:
        print(f"L{i+1}: {repr(line)}")
