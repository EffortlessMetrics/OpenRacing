#!/usr/bin/env python3
"""
Game support matrix validation script.

Scans the codebase for all game telemetry adapters, verifies each has at least
one test file, cross-references documentation, and outputs a status table.

Exit code 1 if any registered game adapter lacks tests.
"""
import os
import re
import sys

REPO_ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
ADAPTERS_SRC = os.path.join(REPO_ROOT, "crates", "telemetry-adapters", "src")
ADAPTERS_TESTS = os.path.join(REPO_ROOT, "crates", "telemetry-adapters", "tests")
DOCS_DIR = os.path.join(REPO_ROOT, "docs")


def parse_adapter_factories():
    """Parse adapter_factories() in lib.rs to get all registered game IDs."""
    lib_path = os.path.join(ADAPTERS_SRC, "lib.rs")
    with open(lib_path, "r", encoding="utf-8") as f:
        content = f.read()

    # Match entries like ("acc", new_acc_adapter)
    pattern = re.compile(r'\("([^"]+)",\s*new_\w+\)')
    return pattern.findall(content)


def discover_adapter_modules():
    """Return set of module names from adapter source files (excluding helpers)."""
    skip = {"lib", "codemasters_shared", "codemasters_udp"}
    modules = set()
    for fname in os.listdir(ADAPTERS_SRC):
        if fname.endswith(".rs"):
            name = fname[:-3]
            if name not in skip:
                modules.add(name)
    return modules


def detect_data_source(module_name):
    """Heuristic detection of the data source type for an adapter module."""
    path = os.path.join(ADAPTERS_SRC, module_name + ".rs")
    if not os.path.isfile(path):
        return "unknown"
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()

    # Order matters — more specific checks first
    if "OutGauge" in content:
        return "UDP (OutGauge)"
    if "FlatBuffer" in content or "flatbuf" in content.lower():
        return "UDP (FlatBuffers)"
    if "Salsa20" in content or "salsa20" in content:
        return "UDP (Encrypted)"
    low = content[:1500].lower()
    if "json" in low and ("udp" in low or "UdpSocket" in content):
        return "UDP (JSON)"
    if any(kw in content for kw in ("SharedMemory", "shared_memory", "OpenFileMapping", "MapViewOfFile")):
        return "Shared Memory"
    if "UdpSocket" in content or "udp" in content[:500].lower():
        return "UDP"
    return "unknown"


def has_inline_tests(module_name):
    """Check if the adapter module has #[cfg(test)] or #[test] inline."""
    path = os.path.join(ADAPTERS_SRC, module_name + ".rs")
    if not os.path.isfile(path):
        return False
    with open(path, "r", encoding="utf-8") as f:
        content = f.read()
    return "#[cfg(test)]" in content or "#[test]" in content


def count_external_test_references(game_id, module_name):
    """Count test files in telemetry-adapters/tests that reference this game."""
    count = 0
    if not os.path.isdir(ADAPTERS_TESTS):
        return count
    search_terms = [game_id, module_name, module_name.replace("_", "")]
    for fname in os.listdir(ADAPTERS_TESTS):
        if not fname.endswith(".rs"):
            continue
        fpath = os.path.join(ADAPTERS_TESTS, fname)
        with open(fpath, "r", encoding="utf-8") as f:
            content = f.read().lower()
        if any(term.lower() in content for term in search_terms):
            count += 1
    return count


def count_dedicated_crate_tests(module_name):
    """Check for a dedicated telemetry-<game> crate with tests."""
    crate_dir = os.path.join(REPO_ROOT, "crates", f"telemetry-{module_name}")
    tests_dir = os.path.join(crate_dir, "tests")
    if os.path.isdir(tests_dir):
        return len([f for f in os.listdir(tests_dir) if f.endswith(".rs")])
    return 0


def find_documentation(game_id, module_name):
    """Check if any doc files reference this game."""
    doc_files = []
    search_terms = [game_id, module_name, module_name.replace("_", " ")]
    for root, _dirs, files in os.walk(DOCS_DIR):
        for fname in files:
            if not fname.endswith(".md"):
                continue
            fpath = os.path.join(root, fname)
            with open(fpath, "r", encoding="utf-8") as f:
                content = f.read().lower()
            if any(term.lower() in content for term in search_terms):
                rel = os.path.relpath(fpath, REPO_ROOT).replace("\\", "/")
                doc_files.append(rel)
    return doc_files


def main():
    game_ids = parse_adapter_factories()
    modules = discover_adapter_modules()

    if not game_ids:
        print("ERROR: No game adapters found in adapter_factories()", file=sys.stderr)
        return 1

    # Build a mapping from game_id to the most likely module name
    # The factory function name pattern is new_<something>_adapter
    lib_path = os.path.join(ADAPTERS_SRC, "lib.rs")
    with open(lib_path, "r", encoding="utf-8") as f:
        lib_content = f.read()

    id_to_module = {}
    for gid in game_ids:
        # Try to find the factory line to derive the module
        pattern = re.compile(
            r'\("' + re.escape(gid) + r'",\s*new_(\w+?)_adapter\)'
        )
        m = pattern.search(lib_content)
        if m:
            factory_stem = m.group(1)
            # The module is typically the factory stem or close to it
            if factory_stem in modules:
                id_to_module[gid] = factory_stem
            else:
                # Try matching game_id directly
                gid_mod = gid.replace("-", "_")
                if gid_mod in modules:
                    id_to_module[gid] = gid_mod
                else:
                    id_to_module[gid] = factory_stem
        else:
            id_to_module[gid] = gid.replace("-", "_")

    # Collect results
    results = []
    any_missing_tests = False

    for gid in game_ids:
        mod_name = id_to_module.get(gid, gid)
        data_source = detect_data_source(mod_name)
        inline = has_inline_tests(mod_name)
        ext_count = count_external_test_references(gid, mod_name)
        crate_tests = count_dedicated_crate_tests(mod_name)
        docs = find_documentation(gid, mod_name)
        total_tests = ext_count + crate_tests + (1 if inline else 0)

        if total_tests == 0:
            any_missing_tests = True

        results.append({
            "game_id": gid,
            "module": mod_name,
            "data_source": data_source,
            "inline_tests": inline,
            "external_test_files": ext_count,
            "crate_test_files": crate_tests,
            "total_test_score": total_tests,
            "docs": docs,
        })

    # Print table
    print()
    print("Game Support Matrix Validation")
    print("=" * 100)
    print(
        f"{'Game ID':<25} {'Data Source':<18} {'Tests':<8} {'Crate':<7} {'Ext':<5} {'Docs':<5} {'Status'}"
    )
    print("-" * 100)

    for r in results:
        test_str = str(r["total_test_score"])
        crate_str = str(r["crate_test_files"]) if r["crate_test_files"] else "-"
        ext_str = str(r["external_test_files"]) if r["external_test_files"] else "-"
        doc_str = "Yes" if r["docs"] else "No"
        status = "OK" if r["total_test_score"] > 0 else "MISSING TESTS"
        print(
            f"{r['game_id']:<25} {r['data_source']:<18} {test_str:<8} {crate_str:<7} {ext_str:<5} {doc_str:<5} {status}"
        )

    print("-" * 100)
    print(f"Total adapters: {len(results)}")

    games_with_tests = sum(1 for r in results if r["total_test_score"] > 0)
    games_without_tests = sum(1 for r in results if r["total_test_score"] == 0)
    print(f"With tests:     {games_with_tests}")
    print(f"Without tests:  {games_without_tests}")
    print()

    if any_missing_tests:
        missing = [r["game_id"] for r in results if r["total_test_score"] == 0]
        print(f"FAIL: {len(missing)} game(s) lack tests: {', '.join(missing)}", file=sys.stderr)
        return 1

    print("PASS: All game adapters have at least one test.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
