#!/usr/bin/env python3
import re, sys, subprocess, os
ROOT = r"H:\Code\Rust\OpenRacing"
os.chdir(ROOT)

CRATES = {
    "telemetry-raceroom": {
        "pkg_name": "racing-wheel-telemetry-raceroom",
        "description": "RaceRoom Racing Experience telemetry adapter (R3E shared memory)",
        "keywords": '["telemetry", "raceroom", "r3e", "racing", "simracing"]',
        "adapter": "RaceRoomAdapter",
        "game_id": "raceroom",
    },
    "telemetry-kartkraft": {
        "pkg_name": "racing-wheel-telemetry-kartkraft",
        "description": "KartKraft telemetry adapter (FlatBuffers UDP)",
        "keywords": '["telemetry", "kartkraft", "flatbuffers", "racing", "simracing"]',
        "adapter": "KartKraftAdapter",
        "game_id": "kartkraft",
    },
    "telemetry-wrc-generations": {
        "pkg_name": "racing-wheel-telemetry-wrc-generations",
        "description": "WRC Generations telemetry adapter (Codemasters Mode 1 UDP)",
        "keywords": '["telemetry", "wrc", "codemasters", "racing", "simracing"]',
        "adapter": "WrcGenerationsAdapter",
        "game_id": "wrc_generations",
    },
    "telemetry-rennsport": {
        "pkg_name": "racing-wheel-telemetry-rennsport",
        "description": "Rennsport telemetry adapter (UDP)",
        "keywords": '["telemetry", "rennsport", "racing", "simracing", "udp"]',
        "adapter": "RennsportAdapter",
        "game_id": "rennsport",
    },
}

for crate_dir, info in CRATES.items():
    src = f"crates/{crate_dir}/src"
    os.makedirs(src, exist_ok=True)
    with open(f"crates/{crate_dir}/Cargo.toml", "w", encoding="utf-8", newline="\n") as f:
        f.write(f'''[package]
name = "{info["pkg_name"]}"
version.workspace = true
edition.workspace = true
license.workspace = true
authors.workspace = true
repository.workspace = true
homepage = "https://github.com/EffortlessMetrics/OpenRacing"
documentation = "https://docs.rs/{info["pkg_name"]}"
publish = true
description = "{info["description"]}"
readme = "README.md"
keywords = {info["keywords"]}

[dependencies]
racing-wheel-telemetry-adapters = {{ path = "../telemetry-adapters" }}
racing-wheel-telemetry-core = {{ path = "../telemetry-core" }}
anyhow = {{ workspace = true }}
async-trait = {{ workspace = true }}
tokio = {{ workspace = true }}
tracing = {{ workspace = true }}
workspace-hack = {{ version = "0.1", path = "../../workspace-hack" }}

[dev-dependencies]
tokio = {{ workspace = true, features = ["rt", "macros"] }}
''')
    mod_name = info["pkg_name"].replace("-", "_")
    with open(f"{src}/lib.rs", "w", encoding="utf-8", newline="\n") as f:
        f.write(f'''#![deny(static_mut_refs)]
pub use racing_wheel_telemetry_adapters::TelemetryAdapter;
pub use racing_wheel_telemetry_adapters::{info["adapter"]};
pub use racing_wheel_telemetry_core::{{NormalizedTelemetry, TelemetryFrame}};
''')
    print(f"Created crates/{crate_dir}/")

# Fix Cargo.toml
with open("Cargo.toml", encoding="utf-8") as f:
    cargo = f.read().lstrip("\ufeff")
for m in CRATES:
    if f'"crates/{m}"' not in cargo:
        cargo = cargo.replace('    "crates/telemetry-ams2",', f'    "crates/telemetry-ams2",\n    "crates/{m}",')
cargo = cargo.replace('    "crates/telemetry-forza",\n', "")
with open("Cargo.toml", "w", encoding="utf-8", newline="\n") as f:
    f.write(cargo)
print("Cargo.toml updated")

# Fix YAML
def fix_yaml(path):
    with open(path, encoding="utf-8") as f:
        lines = f.readlines()
    rr = [i for i,l in enumerate(lines) if re.fullmatch(r"  raceroom:\n", l)]
    if len(rr) >= 2:
        second = rr[1]
        end = second + 1
        while end < len(lines):
            if lines[end].strip() == "" and end+1 < len(lines) and re.match(r"  \w", lines[end+1]):
                break
            end += 1
        lines = lines[:second] + lines[end+1:]
    rr = [i for i,l in enumerate(lines) if re.fullmatch(r"  raceroom:\n", l)]
    if rr:
        r = rr[0]
        fixups = [(r+9, "r3e_shared_memory", '        telemetry_method: "shared_memory_r3e"\n'),
                  (r+20, "r3e_shared_memory", '      method: "shared_memory_r3e"\n'),
                  (r+26, "ffb_scalar:", "        ffb_scalar: null\n"),
                  (r+27, "rpm:", "        rpm: null\n"),
                  (r+28, "speed_ms:", "        speed_ms: null\n"),
                  (r+29, "slip_ratio:", "        slip_ratio: null\n"),
                  (r+30, "gear:", "        gear: null\n"),
                  (r+31, "flags:", "        flags: null\n"),
                  (r+32, "car_id:", "        car_id: null\n"),
                  (r+33, "track_id:", "        track_id: null\n"),
                  (r+43, "raceroom racing experience", '        - "Program Files (x86)/Steam/steamapps/common/Race Room Racing Experience"\n')]
        for idx, old, new in fixups:
            if idx < len(lines) and old in lines[idx]:
                lines[idx] = new
    with open(path, "w", encoding="utf-8", newline="\n") as f:
        f.writelines(lines)
    rr2 = [i for i,l in enumerate(lines) if re.fullmatch(r"  raceroom:\n", l)]
    print(f"  {path}: {len(lines)} lines, raceroom at {rr2}")

fix_yaml("crates/telemetry-config/src/game_support_matrix.yaml")
fix_yaml("crates/telemetry-support/src/game_support_matrix.yaml")
r = subprocess.run(["python", "scripts/sync_yaml.py", "--check"], capture_output=True, text=True)
if r.returncode != 0:
    subprocess.run(["python", "scripts/sync_yaml.py", "--fix"])
    r2 = subprocess.run(["python", "scripts/sync_yaml.py", "--check"], capture_output=True, text=True)
    print("YAML sync after fix:", r2.stdout.strip())
else:
    print("YAML sync:", r.stdout.strip())
print("Done!")