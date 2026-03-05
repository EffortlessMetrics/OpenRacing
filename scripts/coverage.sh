#!/usr/bin/env bash
# coverage.sh — Local code coverage helper for OpenRacing
#
# Usage:
#   ./scripts/coverage.sh            # Generate text summary
#   ./scripts/coverage.sh --html     # Generate HTML report and open it
#   ./scripts/coverage.sh --json     # Generate JSON report (codecov format)
#   ./scripts/coverage.sh --lcov     # Generate LCOV report
#
# Prerequisites:
#   rustup component add llvm-tools-preview
#   cargo install cargo-llvm-cov

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$ROOT_DIR"

# Common arguments shared across all report formats
COMMON_ARGS=(
    --workspace
    --all-features
    --exclude racing-wheel-ui
    --exclude racing-wheel-integration-tests
    --ignore-filename-regex '(\.pb\.rs$|/tests/|/benches/|/fuzz/|/build\.rs$|_test\.rs$|/target/)'
)

check_prerequisites() {
    if ! command -v cargo-llvm-cov &>/dev/null && ! cargo llvm-cov --version &>/dev/null 2>&1; then
        echo "ERROR: cargo-llvm-cov is not installed."
        echo "Install it with: cargo install cargo-llvm-cov"
        echo "Also ensure:     rustup component add llvm-tools-preview"
        exit 1
    fi
}

generate_text() {
    echo "Generating text coverage summary..."
    cargo llvm-cov "${COMMON_ARGS[@]}"
}

generate_html() {
    echo "Generating HTML coverage report..."
    cargo llvm-cov "${COMMON_ARGS[@]}" --html
    local report_dir="target/llvm-cov/html"
    echo "Report generated at: $report_dir/index.html"
    if command -v xdg-open &>/dev/null; then
        xdg-open "$report_dir/index.html"
    elif command -v open &>/dev/null; then
        open "$report_dir/index.html"
    else
        echo "Open $report_dir/index.html in your browser."
    fi
}

generate_json() {
    echo "Generating JSON (codecov) coverage report..."
    cargo llvm-cov "${COMMON_ARGS[@]}" --codecov --output-path codecov.json
    echo "Report written to: codecov.json"
}

generate_lcov() {
    echo "Generating LCOV coverage report..."
    cargo llvm-cov "${COMMON_ARGS[@]}" --lcov --output-path lcov.info
    echo "Report written to: lcov.info"
}

main() {
    check_prerequisites

    case "${1:-}" in
        --html)
            generate_html
            ;;
        --json)
            generate_json
            ;;
        --lcov)
            generate_lcov
            ;;
        *)
            generate_text
            ;;
    esac
}

main "$@"
