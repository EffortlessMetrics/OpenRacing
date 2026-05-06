#!/usr/bin/env python3
"""Prepare all workspace crates for crates.io publication.

This script:
1. Adds `publish = false` to internal/non-publishable crates
2. Adds missing `description` fields
3. Adds missing `publish = true` fields
4. Adds missing `keywords` and `categories`
5. Adds missing `homepage`, `documentation`, `readme` fields
6. Adds `version = "0.1.0"` to all path-only workspace dependencies
"""
import re
import sys
from pathlib import Path

# --------------- configuration ---------------

# Crates that should NOT be published
UNPUBLISHED = {
    "crates/integration-tests",
    "crates/ui",
    "crates/compat",
    "crates/plugin-examples",
    "workspace-hack",  # already has publish = false
}

# Already has publish = false
ALREADY_UNPUBLISHED = {
    "crates/pidff-common",
    "crates/openracing-test-helpers",
    "crates/openracing-atomic/tests/fuzz",
    "workspace-hack",
}

# Descriptions for crates that are missing them
DESCRIPTIONS = {
    "crates/schemas": "Protobuf schemas and domain models for OpenRacing IPC and configuration",
    "crates/ks": "Moza KS steering wheel USB HID protocol parsing and support",
    "crates/srp": "Moza SRP pedals USB HID protocol parsing and support",
    "crates/hbp": "Moza HBP handbrake USB HID protocol parsing and support",
    "crates/moza-wheelbase-report": "Moza wheelbase HID report definitions and parsing",
    "crates/input-maps": "Unified input mapping structures for racing peripherals",
    "crates/hid-moza-protocol": "Moza Racing USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-fanatec-protocol": "Fanatec USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-logitech-protocol": "Logitech USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-thrustmaster-protocol": "Thrustmaster USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-simagic-protocol": "Simagic USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-vrs-protocol": "VRS DirectForce Pro USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-simucube-protocol": "Simucube USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-heusinkveld-protocol": "Heusinkveld USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-asetek-protocol": "Asetek SimSports USB HID protocol constants and encoders (I/O-free, allocation-free)",
    "crates/hid-button-box-protocol": "Generic button box USB HID protocol constants and device classification",
    "crates/simplemotion-v2": "SimpleMotion V2 serial protocol for Granite Devices servo controllers",
    "crates/openracing-capture-ids": "Hardware identifiers and USB VID/PID constants for HID device captures",
    "crates/openracing-profile": "Profile data types and serialization for OpenRacing force feedback tuning",
    "crates/openracing-device-types": "Fundamental device type enumerations and classification for OpenRacing",
    "crates/openracing-hid-common": "Common abstractions for USB HID device communication in OpenRacing",
    "crates/openracing-shifter": "Sequential and H-pattern shifter device types for OpenRacing",
    "crates/openracing-handbrake": "Analog handbrake device types and calibration for OpenRacing",
    "crates/openracing-telemetry-streams": "In-memory telemetry data channels for real-time streaming in OpenRacing",
    "crates/openracing-ffb": "Force feedback core calculations, effects, and torque output for OpenRacing",
    "crates/openracing-calibration": "Axis calibration, deadzones, and normalization for racing peripherals",
    "crates/engine": "Real-time force feedback engine with 1kHz pipeline for OpenRacing",
    "crates/service": "Background service daemon for OpenRacing hardware management",
    "crates/cli": "Command-line tools for configuring and diagnosing OpenRacing",
    "crates/plugins": "Native and WASM plugin runtime for extending OpenRacing",
    # Internal ones getting publish=false don't need descriptions, but add them anyway
    "crates/ui": "Tauri-based desktop GUI for OpenRacing",
    "crates/integration-tests": "Integration and acceptance tests for OpenRacing",
    "crates/compat": "Compatibility layer and snapshot tests for OpenRacing schemas",
}

# Keywords for crates missing them (max 5 per crate)
KEYWORDS = {
    "crates/schemas": ["schemas", "protobuf", "ipc", "openracing", "grpc"],
    "crates/ks": ["moza", "steering-wheel", "hid", "racing", "usb"],
    "crates/srp": ["moza", "pedals", "hid", "racing", "usb"],
    "crates/hbp": ["moza", "handbrake", "hid", "racing", "usb"],
    "crates/moza-wheelbase-report": ["moza", "wheelbase", "hid", "racing", "report"],
    "crates/input-maps": ["input", "mapping", "racing", "controller", "hid"],
    "crates/hid-moza-protocol": ["moza", "hid", "usb", "racing", "protocol"],
    "crates/hid-fanatec-protocol": ["fanatec", "hid", "usb", "racing", "protocol"],
    "crates/hid-logitech-protocol": ["logitech", "hid", "usb", "racing", "protocol"],
    "crates/hid-thrustmaster-protocol": ["thrustmaster", "hid", "usb", "racing", "protocol"],
    "crates/hid-simagic-protocol": ["simagic", "hid", "usb", "racing", "protocol"],
    "crates/hid-vrs-protocol": ["vrs", "hid", "usb", "racing", "protocol"],
    "crates/hid-simucube-protocol": ["simucube", "hid", "usb", "racing", "protocol"],
    "crates/hid-heusinkveld-protocol": ["heusinkveld", "hid", "usb", "racing", "protocol"],
    "crates/hid-asetek-protocol": ["asetek", "hid", "usb", "racing", "protocol"],
    "crates/hid-button-box-protocol": ["button-box", "hid", "usb", "racing", "protocol"],
    "crates/hid-leo-bodnar-protocol": ["leo-bodnar", "hid", "usb", "racing", "protocol"],
    "crates/hid-accuforce-protocol": ["accuforce", "hid", "usb", "racing", "protocol"],
    "crates/hid-openffboard-protocol": ["openffboard", "hid", "usb", "racing", "protocol"],
    "crates/hid-ffbeast-protocol": ["ffbeast", "hid", "usb", "racing", "protocol"],
    "crates/hid-cammus-protocol": ["cammus", "hid", "usb", "racing", "protocol"],
    "crates/hid-cube-controls-protocol": ["cube-controls", "hid", "usb", "racing", "protocol"],
    "crates/hid-pxn-protocol": ["pxn", "hid", "usb", "racing", "protocol"],
    "crates/simplemotion-v2": ["simplemotion", "servo", "serial", "granite", "motor"],
    "crates/openracing-capture-ids": ["capture", "hid", "usb", "vid-pid", "openracing"],
    "crates/openracing-profile": ["profile", "ffb", "tuning", "config", "openracing"],
    "crates/openracing-device-types": ["device", "types", "classification", "hid", "openracing"],
    "crates/openracing-hid-common": ["hid", "usb", "common", "device", "openracing"],
    "crates/openracing-shifter": ["shifter", "racing", "sequential", "h-pattern", "openracing"],
    "crates/openracing-handbrake": ["handbrake", "racing", "analog", "calibration", "openracing"],
    "crates/openracing-telemetry-streams": ["telemetry", "streaming", "channels", "real-time", "openracing"],
    "crates/openracing-ffb": ["ffb", "force-feedback", "torque", "haptics", "openracing"],
    "crates/openracing-calibration": ["calibration", "deadzone", "normalization", "axis", "openracing"],
    "crates/engine": ["engine", "ffb", "real-time", "racing", "openracing"],
    "crates/service": ["service", "daemon", "hardware", "racing", "openracing"],
    "crates/cli": ["cli", "config", "diagnostic", "racing", "openracing"],
    "crates/plugins": ["plugins", "wasm", "native", "extensible", "openracing"],
    "crates/hid-capture": ["hid", "capture", "usb", "replay", "openracing"],
    "crates/openracing-capture-format": ["capture", "hid", "format", "replay", "openracing"],
    "crates/openracing-atomic": ["atomic", "metrics", "real-time", "counters", "openracing"],
    "crates/openracing-scheduler": ["scheduler", "real-time", "pll", "timing", "openracing"],
    "crates/openracing-errors": ["errors", "diagnostics", "openracing", "fault", "safety"],
    "crates/openracing-crypto": ["crypto", "signatures", "verification", "ed25519", "openracing"],
    "crates/openracing-plugin-abi": ["plugin", "abi", "ffi", "versioning", "openracing"],
    "crates/openracing-native-plugin": ["plugin", "native", "dll", "loading", "openracing"],
    "crates/openracing-wasm-runtime": ["wasm", "runtime", "sandbox", "plugin", "openracing"],
    "crates/openracing-fmea": ["fmea", "fault", "safety", "analysis", "openracing"],
    "crates/openracing-watchdog": ["watchdog", "monitoring", "health", "safety", "openracing"],
    "crates/openracing-hardware-watchdog": ["watchdog", "hardware", "torque", "safety", "openracing"],
    "crates/openracing-firmware-update": ["firmware", "update", "ota", "partition", "openracing"],
    "crates/openracing-ipc": ["ipc", "grpc", "transport", "service", "openracing"],
    "crates/openracing-diagnostic": ["diagnostic", "recording", "replay", "debug", "openracing"],
    "crates/openracing-profile-repository": ["profile", "storage", "repository", "persistence", "openracing"],
    "crates/changelog": ["changelog", "parsing", "semver", "release", "openracing"],
}

# Categories for crates missing them
DEFAULT_HID_CATEGORIES = ["hardware-support", "no-std"]
DEFAULT_OPENRACING_CATEGORIES = ["game-development", "hardware-support"]

CATEGORIES = {
    "crates/schemas": ["game-development", "data-structures"],
    "crates/ks": DEFAULT_HID_CATEGORIES,
    "crates/srp": DEFAULT_HID_CATEGORIES,
    "crates/hbp": DEFAULT_HID_CATEGORIES,
    "crates/moza-wheelbase-report": DEFAULT_HID_CATEGORIES,
    "crates/input-maps": ["game-development", "data-structures"],
    "crates/hid-moza-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-fanatec-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-logitech-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-thrustmaster-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-simagic-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-vrs-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-simucube-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-heusinkveld-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-asetek-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-button-box-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-leo-bodnar-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-accuforce-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-openffboard-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-ffbeast-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-cammus-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-cube-controls-protocol": DEFAULT_HID_CATEGORIES,
    "crates/hid-pxn-protocol": DEFAULT_HID_CATEGORIES,
    "crates/simplemotion-v2": ["hardware-support", "embedded"],
    "crates/openracing-capture-ids": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-profile": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-device-types": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-hid-common": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-shifter": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-handbrake": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-telemetry-streams": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-ffb": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-calibration": DEFAULT_OPENRACING_CATEGORIES,
    "crates/engine": ["game-development", "hardware-support", "embedded"],
    "crates/service": DEFAULT_OPENRACING_CATEGORIES,
    "crates/cli": ["command-line-utilities"],
    "crates/plugins": DEFAULT_OPENRACING_CATEGORIES,
    "crates/hid-capture": ["command-line-utilities", "hardware-support"],
    "crates/openracing-capture-format": ["data-structures", "hardware-support"],
    "crates/changelog": ["development-tools", "parsing"],
    # Telemetry crates
    "crates/telemetry-core": ["game-development"],
    "crates/telemetry-config": ["game-development", "config"],
    "crates/telemetry-adapters": ["game-development"],
    "crates/telemetry-lfs": ["game-development"],
    "crates/telemetry-ams2": ["game-development"],
    "crates/telemetry-simhub": ["game-development"],
    "crates/telemetry-mudrunner": ["game-development"],
    "crates/telemetry-f1": ["game-development"],
    "crates/telemetry-forza": ["game-development"],
    "crates/telemetry-rennsport": ["game-development"],
    "crates/telemetry-wrc-generations": ["game-development"],
    "crates/telemetry-kartkraft": ["game-development"],
    "crates/telemetry-raceroom": ["game-development"],
    "crates/telemetry-recorder": ["game-development", "data-structures"],
    "crates/telemetry-config-writers": ["game-development", "config"],
    "crates/telemetry-rate-limiter": ["game-development"],
    "crates/telemetry-orchestrator": ["game-development"],
    "crates/telemetry-bdd-metrics": ["development-tools"],
    "crates/telemetry-support": ["game-development"],
    "crates/telemetry-integration": ["game-development", "development-tools"],
    "crates/telemetry-contracts": ["game-development"],
    "crates/openracing-atomic": ["concurrency", "no-std"],
    "crates/openracing-tracing": ["development-tools::profiling"],
    "crates/openracing-scheduler": ["concurrency", "embedded"],
    "crates/openracing-curves": ["algorithms", "game-development"],
    "crates/openracing-errors": ["rust-patterns"],
    "crates/openracing-pipeline": ["game-development", "algorithms"],  # already has categories
    "crates/openracing-crypto": ["cryptography"],
    "crates/openracing-plugin-abi": ["api-bindings"],
    "crates/openracing-native-plugin": ["api-bindings"],
    "crates/openracing-wasm-runtime": ["wasm"],
    "crates/openracing-fmea": ["development-tools"],
    "crates/openracing-watchdog": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-hardware-watchdog": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-profile-repository": ["config", "filesystem"],  # already has categories
    "crates/openracing-firmware-update": DEFAULT_OPENRACING_CATEGORIES,
    "crates/openracing-ipc": ["network-programming"],
    "crates/openracing-diagnostic": ["development-tools::debugging"],
    "crates/openracing-filters": ["algorithms", "game-development"],  # already has categories
}

# All workspace-local crate names -> their path
# We'll build this from the workspace Cargo.toml
WORKSPACE_VERSION = "0.1.0"

# --------------- helpers ---------------

def read_toml_name(path):
    """Read the package name from a Cargo.toml without a full TOML parser."""
    with open(path, "r", encoding="utf-8") as f:
        for line in f:
            m = re.match(r'^name\s*=\s*"([^"]+)"', line.strip())
            if m:
                return m.group(1)
    return None


def patch_cargo_toml(cargo_path, member, workspace_crate_names):
    """Patch a single Cargo.toml in-place."""
    with open(cargo_path, "r", encoding="utf-8") as f:
        content = f.read()
    
    original = content
    lines = content.split("\n")
    
    # Determine if this crate should be unpublished
    should_unpublish = member in UNPUBLISHED
    already_unpublished = member in ALREADY_UNPUBLISHED
    
    if already_unpublished:
        return False  # skip, already handled
    
    # --- Determine what's present ---
    has_publish = any(re.match(r'^publish\s*=', l.strip()) for l in lines)
    has_description = any(re.match(r'^description\s*=', l.strip()) for l in lines)
    has_keywords = any(re.match(r'^keywords\s*=', l.strip()) for l in lines)
    has_categories = any(re.match(r'^categories\s*=', l.strip()) for l in lines)
    has_homepage = any(re.match(r'^homepage\s*=', l.strip()) for l in lines)
    has_documentation = any(re.match(r'^documentation\s*=', l.strip()) for l in lines)
    has_readme = any(re.match(r'^readme\s*=', l.strip()) for l in lines)
    
    # Find the end of [package] section (before next [section] or certain keys)
    pkg_end_line = None
    in_package = False
    for i, line in enumerate(lines):
        stripped = line.strip()
        if stripped == "[package]":
            in_package = True
            continue
        if in_package and stripped.startswith("[") and not stripped.startswith("[package."):
            pkg_end_line = i
            break
    
    if pkg_end_line is None:
        # Package section extends to end or next section
        # Find the line after the last package-level key
        for i, line in enumerate(lines):
            stripped = line.strip()
            if stripped.startswith("[") and stripped != "[package]" and not stripped.startswith("[package."):
                pkg_end_line = i
                break
        if pkg_end_line is None:
            pkg_end_line = len(lines)
    
    # Read the package name from the file
    pkg_name = read_toml_name(cargo_path)
    
    # Build lines to insert before pkg_end_line
    insert_lines = []
    
    if should_unpublish:
        if not has_publish:
            insert_lines.append('publish = false')
        else:
            # Replace existing publish = true with false
            for i, line in enumerate(lines):
                if re.match(r'^publish\s*=\s*true', line.strip()):
                    lines[i] = 'publish = false'
    else:
        if not has_publish:
            insert_lines.append('publish = true')
        
        if not has_homepage:
            insert_lines.append('homepage = "https://github.com/EffortlessMetrics/OpenRacing"')
        
        if not has_documentation and pkg_name:
            insert_lines.append(f'documentation = "https://docs.rs/{pkg_name}"')
        
        if not has_readme:
            insert_lines.append('readme = "README.md"')
    
    if not has_description and member in DESCRIPTIONS:
        insert_lines.append(f'description = "{DESCRIPTIONS[member]}"')
    
    if not should_unpublish:
        if not has_keywords and member in KEYWORDS:
            kw_str = ", ".join(f'"{k}"' for k in KEYWORDS[member])
            insert_lines.append(f'keywords = [{kw_str}]')
        
        if not has_categories and member in CATEGORIES:
            cat_str = ", ".join(f'"{c}"' for c in CATEGORIES[member])
            insert_lines.append(f'categories = [{cat_str}]')
    
    if insert_lines:
        # Insert before pkg_end_line
        for j, ins_line in enumerate(insert_lines):
            lines.insert(pkg_end_line + j, ins_line)
        # Adjust pkg_end_line for subsequent operations
        pkg_end_line += len(insert_lines)
    
    content = "\n".join(lines)
    
    # --- Add version to path-only workspace dependencies ---
    if not should_unpublish:
        # Match patterns like:
        #   some-crate = { path = "../some-dir" }
        #   some-crate = { path = "../some-dir", optional = true }
        # But NOT if it already has version or workspace
        def add_version_to_path_dep(match):
            full = match.group(0)
            dep_name = match.group(1)
            rest = match.group(2)
            
            # Skip if it already has 'version' or 'workspace' 
            if 'version' in rest or 'workspace' in rest:
                return full
            
            # Only add version for workspace-local crates
            if dep_name not in workspace_crate_names:
                return full
            
            # Insert version after path
            path_match = re.search(r'(path\s*=\s*"[^"]+")', rest)
            if path_match:
                path_str = path_match.group(1)
                new_rest = rest.replace(path_str, f'{path_str}, version = "{WORKSPACE_VERSION}"')
                return f'{dep_name} = {new_rest}'
            return full
        
        # This regex matches dep = { path = "..." ... }
        content = re.sub(
            r'^([\w-]+)\s*=\s*(\{[^}]*path\s*=\s*"[^"]*"[^}]*\})',
            add_version_to_path_dep,
            content,
            flags=re.MULTILINE
        )
    
    if content != original:
        with open(cargo_path, "w", encoding="utf-8", newline="\n") as f:
            f.write(content)
        return True
    return False


def main():
    root = Path(".")
    workspace_toml = root / "Cargo.toml"
    
    if not workspace_toml.exists():
        print("ERROR: Run from workspace root", file=sys.stderr)
        sys.exit(1)
    
    # Read workspace members
    with open(workspace_toml, "r", encoding="utf-8") as f:
        ws_content = f.read()
    
    members = re.findall(r'"([^"]+)"', 
                         re.search(r'members\s*=\s*\[(.*?)\]', ws_content, re.DOTALL).group(1))
    
    # Build map of crate names to paths
    workspace_crate_names = set()
    for member in members:
        cargo_path = root / member / "Cargo.toml"
        if cargo_path.exists():
            name = read_toml_name(cargo_path)
            if name:
                workspace_crate_names.add(name)
    
    # Also add the workspace-hack
    workspace_crate_names.add("workspace-hack")
    
    modified = 0
    for member in members:
        cargo_path = root / member / "Cargo.toml"
        if not cargo_path.exists():
            print(f"  SKIP (missing): {member}")
            continue
        
        if patch_cargo_toml(cargo_path, member, workspace_crate_names):
            print(f"  PATCHED: {member}")
            modified += 1
        else:
            print(f"  OK: {member}")
    
    print(f"\nModified {modified} Cargo.toml files")


if __name__ == "__main__":
    main()
