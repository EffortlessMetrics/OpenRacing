#!/usr/bin/env python3
"""
check_yaml_sync.py — Verify that two game support matrix YAML files are identical.

Usage:
    python scripts/check_yaml_sync.py <file_a> <file_b>

Exits 0 if the files are structurally identical, 1 if they differ.
"""

import sys
import difflib


def load_yaml(path: str):
    try:
        import yaml  # type: ignore
    except ImportError:
        print("ERROR: PyYAML not installed. Run: pip install pyyaml", file=sys.stderr)
        sys.exit(2)

    with open(path, encoding="utf-8") as fh:
        return yaml.safe_load(fh)


def sorted_yaml(obj):
    """Recursively sort dict keys so comparison is order-independent."""
    if isinstance(obj, dict):
        return {k: sorted_yaml(v) for k, v in sorted(obj.items())}
    if isinstance(obj, list):
        # Lists are order-sensitive (e.g. supported_fields), keep order.
        return [sorted_yaml(item) for item in obj]
    return obj


def render_games(data) -> list[str]:
    """Return sorted list of 'key: name' strings for each game entry."""
    games = data.get("games") or {}
    lines = []
    for key in sorted(games):
        name = (games[key] or {}).get("name", key)
        lines.append(f"{key}: {name}")
    return lines


def main() -> int:
    if len(sys.argv) != 3:
        print(f"Usage: {sys.argv[0]} <file_a> <file_b>", file=sys.stderr)
        return 2

    path_a, path_b = sys.argv[1], sys.argv[2]

    try:
        data_a = load_yaml(path_a)
        data_b = load_yaml(path_b)
    except FileNotFoundError as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 2

    norm_a = sorted_yaml(data_a)
    norm_b = sorted_yaml(data_b)

    if norm_a == norm_b:
        print(f"OK: {path_a} and {path_b} are identical.")
        return 0

    # Build a human-readable diff of the game lists.
    games_a = render_games(data_a)
    games_b = render_games(data_b)

    only_a = sorted(set(games_a) - set(games_b))
    only_b = sorted(set(games_b) - set(games_a))

    print("ERROR: game support matrix files are out of sync!", file=sys.stderr)
    print(f"  {path_a}", file=sys.stderr)
    print(f"  {path_b}", file=sys.stderr)
    print("", file=sys.stderr)

    if only_a:
        print(f"Games only in {path_a}:", file=sys.stderr)
        for g in only_a:
            print(f"  + {g}", file=sys.stderr)

    if only_b:
        print(f"Games only in {path_b}:", file=sys.stderr)
        for g in only_b:
            print(f"  + {g}", file=sys.stderr)

    if not only_a and not only_b:
        # Same game keys but differing content — show a structured diff.
        import yaml  # type: ignore

        text_a = yaml.dump(norm_a, default_flow_style=False, sort_keys=True).splitlines()
        text_b = yaml.dump(norm_b, default_flow_style=False, sort_keys=True).splitlines()
        diff = difflib.unified_diff(text_a, text_b, fromfile=path_a, tofile=path_b, lineterm="")
        print("", file=sys.stderr)
        print("Content diff:", file=sys.stderr)
        for line in diff:
            print(line, file=sys.stderr)

    print("", file=sys.stderr)
    print("Fix: update both files to match, or run the single-source-of-truth", file=sys.stderr)
    print("     generator once it is available (see docs/FRICTION_LOG.md F-001).", file=sys.stderr)
    return 1


if __name__ == "__main__":
    sys.exit(main())
