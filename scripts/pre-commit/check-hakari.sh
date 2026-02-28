# scripts/pre-commit/check-hakari.sh
# Standalone workspace-hack verification script.
# Called by .githooks/pre-commit and by CI.
# Usage: bash scripts/pre-commit/check-hakari.sh

set -euo pipefail

if ! command -v cargo-hakari &>/dev/null; then
    echo "cargo-hakari not installed; skipping."
    exit 0
fi

echo "Verifying workspace-hack..."
cargo hakari verify && echo "OK" || {
    echo ""
    echo "workspace-hack is out of date."
    echo "Run: cargo hakari generate"
    exit 1
}
