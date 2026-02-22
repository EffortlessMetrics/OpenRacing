#!/bin/bash
set -euo pipefail

# Script to check protobuf schema compatibility using buf
# This should be run in CI to prevent breaking changes

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
SCHEMAS_DIR="$(dirname "$SCRIPT_DIR")"

cd "$SCHEMAS_DIR"

echo "Checking protobuf schema compatibility..."

# Check if buf is installed
if ! command -v buf &> /dev/null; then
    echo "Error: buf is not installed. Please install buf CLI tool."
    echo "See: https://docs.buf.build/installation"
    exit 1
fi

# Lint the protobuf files
echo "Running buf lint..."
# Emit rule IDs in JSON for diagnostics, then show text output
buf lint --error-format json 2>&1 | python3 -c "import sys,json; [print(f'{e[\"path\"]}:{e[\"start_line\"]}:{e[\"start_column\"]}:[{e.get(\"type\",\"?\")}] {e[\"message\"]}') for line in sys.stdin for e in [json.loads(line)] if line.strip()]" || true
buf lint

# Check for breaking changes against main branch
if git rev-parse --verify origin/main >/dev/null 2>&1; then
    echo "Checking for breaking changes against origin/main..."
    # Use ../../.git because this script runs from crates/schemas/ (not repo root)
    buf breaking --against '../../.git#branch=origin/main,subdir=crates/schemas'
else
    echo "Warning: origin/main not found, skipping breaking change detection"
fi

# Generate code to verify it compiles
echo "Generating protobuf code..."
buf generate

echo "Schema compatibility check completed successfully!"