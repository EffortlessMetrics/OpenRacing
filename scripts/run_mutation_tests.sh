#!/usr/bin/env bash
# Run mutation tests for the safety-critical engine code.
#
# Usage:
#   ./scripts/run_mutation_tests.sh [--jobs N] [--timeout S]
#
# Prerequisites:
#   cargo install cargo-mutants
#
# Exits 1 if any mutants survive (suitable for CI gates).

set -euo pipefail

JOBS="${MUTATION_JOBS:-4}"
TIMEOUT="${MUTATION_TIMEOUT:-60}"
OUTPUT_DIR="${MUTATION_OUTPUT_DIR:-/tmp/mutants-out}"

while [[ $# -gt 0 ]]; do
    case "$1" in
        --jobs)    JOBS="$2";    shift 2 ;;
        --timeout) TIMEOUT="$2"; shift 2 ;;
        --output)  OUTPUT_DIR="$2"; shift 2 ;;
        *) echo "Unknown option: $1" >&2; exit 1 ;;
    esac
done

echo "=== Mutation Testing: racing-wheel-engine (safety-critical) ==="
echo "  Jobs:    $JOBS"
echo "  Timeout: ${TIMEOUT}s per mutant"
echo "  Output:  $OUTPUT_DIR"
echo ""

cargo mutants \
    --package racing-wheel-engine \
    --test-timeout "$TIMEOUT" \
    --jobs "$JOBS" \
    --output "$OUTPUT_DIR"

EXIT_CODE=$?

SUMMARY="${OUTPUT_DIR}/mutants.out/outcomes.json"
if [[ -f "$SUMMARY" ]]; then
    echo ""
    echo "=== Summary ==="
    # Print survived mutant count if jq is available
    if command -v jq &>/dev/null; then
        SURVIVED=$(jq '[.[] | select(.summary == "survived")] | length' "$SUMMARY" 2>/dev/null || echo "?")
        CAUGHT=$(jq   '[.[] | select(.summary == "caught")]   | length' "$SUMMARY" 2>/dev/null || echo "?")
        TIMEOUT_N=$(jq '[.[] | select(.summary == "timeout")] | length' "$SUMMARY" 2>/dev/null || echo "?")
        echo "  Caught:   $CAUGHT"
        echo "  Survived: $SURVIVED"
        echo "  Timeout:  $TIMEOUT_N"
        if [[ "$SURVIVED" != "0" && "$SURVIVED" != "?" ]]; then
            echo ""
            echo "ERROR: $SURVIVED mutant(s) survived — add tests to kill them."
            exit 1
        fi
    fi
fi

exit $EXIT_CODE
