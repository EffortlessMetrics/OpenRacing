#!/usr/bin/env python3
"""
sync_yaml.py — Keep the two game support matrix YAML files in sync.

The canonical source is:
    crates/telemetry-config/src/game_support_matrix.yaml

The mirror is:
    crates/telemetry-support/src/game_support_matrix.yaml

Usage:
    python scripts/sync_yaml.py --check   # Exit 1 if files differ (no writes)
    python scripts/sync_yaml.py --fix     # Copy canonical → mirror

Requires Python 3.8+. No external dependencies.
"""

import argparse
import difflib
import shutil
import sys
from pathlib import Path

CANONICAL = Path("crates/telemetry-config/src/game_support_matrix.yaml")
MIRROR = Path("crates/telemetry-support/src/game_support_matrix.yaml")


def read_lines(path: Path) -> list[str]:
    try:
        return path.read_text(encoding="utf-8").splitlines(keepends=True)
    except FileNotFoundError:
        print(f"ERROR: file not found: {path}", file=sys.stderr)
        sys.exit(2)


def show_diff(lines_a: list[str], lines_b: list[str]) -> None:
    diff = difflib.unified_diff(
        lines_a,
        lines_b,
        fromfile=str(CANONICAL),
        tofile=str(MIRROR),
        lineterm="",
    )
    print("Diff (canonical → mirror):", file=sys.stderr)
    for line in diff:
        print(line, file=sys.stderr)


def main() -> int:
    parser = argparse.ArgumentParser(
        description="Keep game support matrix YAML files in sync.",
    )
    mode = parser.add_mutually_exclusive_group(required=True)
    mode.add_argument(
        "--check",
        action="store_true",
        help="Exit 1 if files differ without modifying anything.",
    )
    mode.add_argument(
        "--fix",
        action="store_true",
        help="Copy canonical (telemetry-config) to mirror (telemetry-support).",
    )
    args = parser.parse_args()

    lines_canonical = read_lines(CANONICAL)
    lines_mirror = read_lines(MIRROR)

    if lines_canonical == lines_mirror:
        print(f"OK: {CANONICAL} and {MIRROR} are identical.")
        return 0

    # Files differ — report it.
    print("ERROR: game support matrix files are out of sync!", file=sys.stderr)
    print(f"  canonical : {CANONICAL}", file=sys.stderr)
    print(f"  mirror    : {MIRROR}", file=sys.stderr)
    print("", file=sys.stderr)
    show_diff(lines_canonical, lines_mirror)
    print("", file=sys.stderr)

    if args.check:
        print(
            "Run `python scripts/sync_yaml.py --fix` to copy the canonical version to the mirror.",
            file=sys.stderr,
        )
        return 1

    # --fix: overwrite mirror with canonical.
    shutil.copy2(CANONICAL, MIRROR)
    print(f"Fixed: copied {CANONICAL} → {MIRROR}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
