#!/usr/bin/env python3
"""Fix all files for the new telemetry adapters task."""
import re
import sys
import subprocess

ROOT = r"H:\Code\Rust\OpenRacing"
import os
os.chdir(ROOT)

# ── 1. Fix Cargo.toml ────────────────────────────────────────────────────────
CARGO_TOML = "Cargo.toml"
with open(CARGO_TOML, encoding="utf-8") as f:
    cargo = f.read()

# Remove BOM if present
cargo = cargo.lstrip("\ufeff")

NEW_MEMBERS = [
    "telemetry-raceroom",
    "telemetry-kartkraft",
    "telemetry-wrc-generations",
    "telemetry-rennsport",
]
missing = [m for m in NEW_MEMBERS if f'"crates/{m}"' not in cargo]

if missing:
    # Insert after telemetry-ams2
    ams2_line = '    "crates/telemetry-ams2",'
    insert = ams2_line + "\n" + "\n".join(f'    "crates/{m}",' for m in missing)
    if ams2_line in cargo:
        cargo = cargo.replace(ams2_line, insert, 1)
        print(f"Cargo.toml: inserted {missing}")
    else:
        print("ERROR: Could not find telemetry-ams2 line in Cargo.toml")
        sys.exit(1)
else:
    print(f"Cargo.toml: all new crates already present")

# Remove telemetry-forza if present (crate dir doesn't exist)
if '"crates/telemetry-forza"' in cargo:
    cargo = cargo.replace('    "crates/telemetry-forza",\n', "")
    print("Cargo.toml: removed telemetry-forza (crate doesn't exist)")

with open(CARGO_TOML, "w", encoding="utf-8", newline="\n") as f:
    f.write(cargo)
print(f"Cargo.toml: written ({len(cargo)} bytes)")

# Verify
with open(CARGO_TOML, encoding="utf-8") as f:
    verify = f.read()
for m in NEW_MEMBERS:
    status = "✓" if f'"crates/{m}"' in verify else "✗"
    print(f"  {status} {m}")
forza_status = "✗ (STILL PRESENT)" if '"crates/telemetry-forza"' in verify else "✓ removed"
print(f"  telemetry-forza: {forza_status}")

print()

# ── 2. Fix YAML files ─────────────────────────────────────────────────────────
def fix_raceroom_yaml(path):
    with open(path, encoding="utf-8") as f:
        lines = f.readlines()

    rr_indices = [i for i, l in enumerate(lines) if re.fullmatch(r"  raceroom:\n", l)]
    print(f"{path}: {len(lines)} lines, raceroom at: {rr_indices}")

    if len(rr_indices) == 1:
        print(f"  Already has single raceroom entry. Checking for needed fixes...")
        rr = rr_indices[0]
    elif len(rr_indices) == 2:
        print(f"  Has duplicate raceroom entries. Fixing...")
        rr = rr_indices[0]  # keep first
        second = rr_indices[1]
        # Delete second entry (and its trailing blank)
        end = second + 1
        while end < len(lines):
            if lines[end].strip() == "" and end + 1 < len(lines) and re.match(r"  \w", lines[end + 1]):
                break
            end += 1
        lines = lines[:second] + lines[end + 1:]
        print(f"  Removed second entry (lines {second+1} to {end+1})")
    else:
        print(f"  ERROR: unexpected raceroom count: {len(rr_indices)}")
        return

    # Fix the first entry
    changed = []

    def fix_line(idx, old_substr, new_line):
        nonlocal lines
        if idx < len(lines) and old_substr in lines[idx]:
            lines[idx] = new_line
            changed.append(f"    line {idx+1}: {new_line.rstrip()}")

    fix_line(rr + 9, "r3e_shared_memory", '        telemetry_method: "shared_memory_r3e"\n')
    fix_line(rr + 20, "r3e_shared_memory", '      method: "shared_memory_r3e"\n')
    fix_line(rr + 26, "ffb_scalar:", "        ffb_scalar: null\n")
    fix_line(rr + 27, "rpm:", "        rpm: null\n")
    fix_line(rr + 28, "speed_ms:", "        speed_ms: null\n")
    fix_line(rr + 29, "slip_ratio:", "        slip_ratio: null\n")
    fix_line(rr + 30, "gear:", "        gear: null\n")
    fix_line(rr + 31, "flags:", "        flags: null\n")
    fix_line(rr + 32, "car_id:", "        car_id: null\n")
    fix_line(rr + 33, "track_id:", "        track_id: null\n")
    fix_line(rr + 43, "raceroom racing experience",
             '        - "Program Files (x86)/Steam/steamapps/common/Race Room Racing Experience"\n')

    if changed:
        for c in changed:
            print(c)
    else:
        print("  No line-level fixes needed.")

    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.writelines(lines)

    # Verify
    with open(path, encoding="utf-8") as f:
        v = f.readlines()
    rr_v = [i for i, l in enumerate(v) if re.fullmatch(r"  raceroom:\n", l)]
    print(f"  After: {len(v)} lines, raceroom at: {rr_v}")
    print()


fix_raceroom_yaml("crates/telemetry-config/src/game_support_matrix.yaml")
fix_raceroom_yaml("crates/telemetry-support/src/game_support_matrix.yaml")

# ── 3. Verify sync ────────────────────────────────────────────────────────────
result = subprocess.run(
    ["python", "scripts/sync_yaml.py", "--check"],
    capture_output=True, text=True
)
print("sync_yaml.py --check:", result.stdout.strip() or result.stderr.strip())
if result.returncode != 0:
    # Try fix
    fix_result = subprocess.run(
        ["python", "scripts/sync_yaml.py", "--fix"],
        capture_output=True, text=True
    )
    print("sync_yaml.py --fix:", fix_result.stdout.strip())
    check2 = subprocess.run(
        ["python", "scripts/sync_yaml.py", "--check"],
        capture_output=True, text=True
    )
    print("sync_yaml.py --check (after fix):", check2.stdout.strip())

print("\nAll done!")
