#!/bin/bash
# Reproducible build script for Racing Wheel Suite

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${OUTPUT_DIR:-$PROJECT_ROOT/dist}"

log_info() { echo "[INFO] $1"; }
log_step() { echo "[STEP] $1"; }

# Parse arguments
TARGETS=()
while [[ $# -gt 0 ]]; do
    case $1 in
        --target) TARGETS+=("$2"); shift 2 ;;
        --all-targets) TARGETS=("x86_64-pc-windows-msvc" "x86_64-unknown-linux-gnu"); shift ;;
        *) shift ;;
    esac
done

# Default target
if [ ${#TARGETS[@]} -eq 0 ]; then
    TARGETS=("x86_64-unknown-linux-gnu")
fi

# Setup environment
setup_environment() {
    log_step "Setting up reproducible build environment"
    export SOURCE_DATE_EPOCH="1640995200"
    export RUSTFLAGS="-C debuginfo=0 -C strip=symbols"
    rm -rf "$OUTPUT_DIR"
    mkdir -p "$OUTPUT_DIR"
}

# Build target
build_target() {
    local target="$1"
    log_step "Building for target: $target"
    
    cd "$PROJECT_ROOT"
    cargo build --release --target "$target" --locked
    
    local target_dir="$OUTPUT_DIR/$target"
    mkdir -p "$target_dir"
    
    local binary_ext=""
    [[ "$target" == *"windows"* ]] && binary_ext=".exe"
    
    for binary in wheeld wheelctl wheel-ui; do
        local src="target/$target/release/${binary}${binary_ext}"
        local dst="$target_dir/${binary}${binary_ext}"
        [[ -f "$src" ]] && cp "$src" "$dst"
    done
}

# Main execution
main() {
    log_info "Starting reproducible build"
    setup_environment
    
    for target in "${TARGETS[@]}"; do
        build_target "$target"
    done
    
    log_info "Build completed: $OUTPUT_DIR"
}

main "$@"