#!/usr/bin/env python3
"""Validate udev rules against HID protocol crate VID/PID constants.

Cross-references packaging/linux/99-racing-wheel-suite.rules against all
VID/PID constants defined in crates/hid-*-protocol/src/ to ensure every
known device is covered.

Usage:
    python scripts/validate_udev_rules.py [--verbose]
"""

import argparse
import os
import re
import sys
from pathlib import Path
from typing import Dict, List, Set, Tuple

REPO_ROOT = Path(__file__).resolve().parent.parent
UDEV_RULES_PATH = REPO_ROOT / "packaging" / "linux" / "99-racing-wheel-suite.rules"
CRATES_DIR = REPO_ROOT / "crates"

# Regex to extract VID/PID pairs from udev rules
# Matches: ATTRS{idVendor}=="xxxx", ATTRS{idProduct}=="xxxx"  (hidraw rules)
UDEV_VID_PID_RE = re.compile(
    r'ATTRS\{idVendor\}=="([0-9a-fA-F]{4})".*?ATTRS\{idProduct\}=="([0-9a-fA-F]{4})"'
)
# Matches vendor-wide rules: ATTRS{idVendor}=="xxxx" without idProduct
UDEV_VID_ONLY_RE = re.compile(
    r'SUBSYSTEM=="hidraw".*ATTRS\{idVendor\}=="([0-9a-fA-F]{4})"'
    r'(?!.*ATTRS\{idProduct\})'
)

# Regex to extract Rust hex constants: `pub const NAME: u16 = 0xXXXX;`
RUST_CONST_RE = re.compile(
    r'pub\s+const\s+(\w+)\s*:\s*u16\s*=\s*0x([0-9a-fA-F]+)\s*;'
)


def parse_udev_rules(path: Path) -> Tuple[Set[Tuple[str, str]], Set[str]]:
    """Parse udev rules file, returning (vid_pid_pairs, vendor_wide_vids)."""
    vid_pid_pairs: Set[Tuple[str, str]] = set()
    vendor_wide_vids: Set[str] = set()

    if not path.exists():
        print(f"ERROR: udev rules file not found: {path}", file=sys.stderr)
        return vid_pid_pairs, vendor_wide_vids

    text = path.read_text(encoding="utf-8")
    for line in text.splitlines():
        line = line.strip()
        if not line or line.startswith("#"):
            continue
        # Check for VID+PID rules
        m = UDEV_VID_PID_RE.search(line)
        if m:
            vid = m.group(1).lower()
            pid = m.group(2).lower()
            vid_pid_pairs.add((vid, pid))
            continue
        # Check for vendor-wide rules (no PID filter)
        m = UDEV_VID_ONLY_RE.search(line)
        if m:
            vendor_wide_vids.add(m.group(1).lower())

    return vid_pid_pairs, vendor_wide_vids


def extract_vid_pids_from_crate(crate_dir: Path) -> Dict[str, List[Tuple[str, str, str]]]:
    """Extract VID/PID constants from a crate's src/ directory.

    Returns dict mapping crate name to list of (vid_hex, pid_hex, const_name).
    VID is inferred from VENDOR_ID constants in the same file.
    """
    results: Dict[str, List[Tuple[str, str, str]]] = {}
    crate_name = crate_dir.name
    src_dir = crate_dir / "src"
    if not src_dir.exists():
        return results

    entries: List[Tuple[str, str, str]] = []

    for rs_file in sorted(src_dir.rglob("*.rs")):
        text = rs_file.read_text(encoding="utf-8")
        consts: Dict[str, str] = {}
        for m in RUST_CONST_RE.finditer(text):
            name = m.group(1)
            value = m.group(2).lower().zfill(4)
            consts[name] = value

        # Identify VID constants (names containing VENDOR_ID)
        vids = {
            name: val for name, val in consts.items()
            if "VENDOR_ID" in name.upper()
        }
        # Identify PID constants (names containing PID or PRODUCT)
        pids = {
            name: val for name, val in consts.items()
            if ("PID" in name.upper() or "PRODUCT" in name.upper())
            and "VENDOR" not in name.upper()
        }

        # If we have VIDs and PIDs in the same file, pair them
        if vids and pids:
            for pid_name, pid_val in pids.items():
                # Determine which VID this PID belongs to
                # Use the primary/first VID unless the PID name hints at a specific one
                vid_val = None
                pid_upper = pid_name.upper()
                for vid_name, vval in vids.items():
                    vid_upper = vid_name.upper()
                    if "LEGACY" in pid_upper and "LEGACY" in vid_upper:
                        vid_val = vval
                        break
                    if "HANDBRAKE_V1" in pid_upper and "HANDBRAKE_V1" in vid_upper:
                        vid_val = vval
                        break
                    if "SHIFTER" in pid_upper and "SHIFTER" in vid_upper:
                        vid_val = vval
                        break
                if vid_val is None:
                    # Use the primary VID (first one without LEGACY/HANDBRAKE/SHIFTER)
                    primary_vids = [
                        v for n, v in vids.items()
                        if "LEGACY" not in n.upper()
                        and "HANDBRAKE" not in n.upper()
                        and "SHIFTER" not in n.upper()
                    ]
                    vid_val = primary_vids[0] if primary_vids else list(vids.values())[0]
                entries.append((vid_val, pid_val, pid_name))
        elif pids and not vids:
            # PIDs without VID in this file — skip (need VID from another file)
            pass

    if entries:
        results[crate_name] = entries
    return results


def scan_all_hid_crates() -> Dict[str, List[Tuple[str, str, str]]]:
    """Scan all HID protocol crates for VID/PID constants."""
    all_entries: Dict[str, List[Tuple[str, str, str]]] = {}
    for crate_dir in sorted(CRATES_DIR.iterdir()):
        if crate_dir.is_dir() and crate_dir.name.startswith("hid-") and "protocol" in crate_dir.name:
            entries = extract_vid_pids_from_crate(crate_dir)
            all_entries.update(entries)
    return all_entries


def validate(verbose: bool = False) -> int:
    """Run the cross-reference validation. Returns 0 on success, 1 on failure."""
    vid_pid_pairs, vendor_wide_vids = parse_udev_rules(UDEV_RULES_PATH)
    crate_entries = scan_all_hid_crates()

    if verbose:
        print(f"Parsed {len(vid_pid_pairs)} VID/PID pairs from udev rules")
        print(f"Found {len(vendor_wide_vids)} vendor-wide VID rules")
        print(f"Scanned {len(crate_entries)} HID protocol crates")
        print()

    missing: List[Tuple[str, str, str, str]] = []  # (crate, vid, pid, const_name)

    for crate_name, entries in sorted(crate_entries.items()):
        # Collect all VIDs used by this crate
        crate_vids = set(vid for vid, _, _ in entries)

        for vid, pid, const_name in entries:
            # Check if covered by vendor-wide rule
            if vid in vendor_wide_vids:
                if verbose:
                    print(f"  OK (vendor-wide): {crate_name} {vid}:{pid} ({const_name})")
                continue
            # Check if covered by specific VID/PID rule
            if (vid, pid) in vid_pid_pairs:
                if verbose:
                    print(f"  OK: {crate_name} {vid}:{pid} ({const_name})")
                continue
            # For multi-VID crates, check if the PID is covered under ANY VID
            found_alt = False
            for alt_vid in crate_vids:
                if alt_vid in vendor_wide_vids or (alt_vid, pid) in vid_pid_pairs:
                    found_alt = True
                    if verbose:
                        print(f"  OK (alt VID {alt_vid}): {crate_name} {vid}:{pid} ({const_name})")
                    break
            if found_alt:
                continue
            missing.append((crate_name, vid, pid, const_name))

    if missing:
        print(f"\nERROR: {len(missing)} VID/PID pair(s) missing from udev rules:\n")
        for crate_name, vid, pid, const_name in missing:
            print(f"  {crate_name}: {vid}:{pid} ({const_name})")
        print(f"\nFile: {UDEV_RULES_PATH}")
        return 1

    total = sum(len(entries) for entries in crate_entries.values())
    print(f"OK: All {total} VID/PID pairs from {len(crate_entries)} HID crates are covered in udev rules.")
    return 0


def validate_syntax() -> int:
    """Validate basic udev rules syntax."""
    if not UDEV_RULES_PATH.exists():
        print(f"ERROR: udev rules file not found: {UDEV_RULES_PATH}", file=sys.stderr)
        return 1

    errors = 0
    text = UDEV_RULES_PATH.read_text(encoding="utf-8")
    for lineno, line in enumerate(text.splitlines(), 1):
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        # Basic syntax checks
        if "==" not in stripped and "+=" not in stripped and "=" not in stripped:
            print(f"  Line {lineno}: no assignment operator found: {stripped[:80]}")
            errors += 1
        # Check for common mistakes
        if 'ATTRS{idVendor}=' in stripped and 'ATTRS{idVendor}==' not in stripped:
            # Single = on ATTRS is wrong (should be ==)
            if 'ATTRS{idVendor}="' in stripped:
                print(f"  Line {lineno}: ATTRS{{idVendor}} uses = instead of ==")
                errors += 1
        # Validate hex VID format (4 hex chars)
        for m in re.finditer(r'ATTRS\{idVendor\}=="([^"]*)"', stripped):
            val = m.group(1)
            if not re.match(r'^[0-9a-fA-F]{4}$', val):
                print(f"  Line {lineno}: invalid VID format: {val}")
                errors += 1
        # Validate hex PID format (4 hex chars)
        for m in re.finditer(r'ATTRS\{idProduct\}=="([^"]*)"', stripped):
            val = m.group(1)
            if not re.match(r'^[0-9a-fA-F]{4}$', val):
                print(f"  Line {lineno}: invalid PID format: {val}")
                errors += 1

    if errors:
        print(f"\nERROR: {errors} syntax error(s) in udev rules")
        return 1
    print("OK: udev rules syntax is valid.")
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description="Validate udev rules against HID crate VID/PIDs")
    parser.add_argument("--verbose", "-v", action="store_true", help="Show detailed output")
    args = parser.parse_args()

    print("=== Validating udev rules syntax ===")
    rc1 = validate_syntax()
    print()
    print("=== Cross-referencing VID/PIDs ===")
    rc2 = validate(verbose=args.verbose)

    return max(rc1, rc2)


if __name__ == "__main__":
    sys.exit(main())
