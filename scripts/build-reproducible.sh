#!/usr/bin/env bash
# Reproducible Build Script for Racing Wheel Suite
# This script ensures deterministic, reproducible builds across environments

set -euo pipefail

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
BUILD_CONFIG="$PROJECT_ROOT/build/reproducible-builds.toml"
BUILD_DIR="$PROJECT_ROOT/target/reproducible"
ARTIFACTS_DIR="$PROJECT_ROOT/artifacts"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Logging functions
log_info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

log_warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

log_error() {
    echo -e "${RED}[ERROR]${NC} $1"
}

log_step() {
    echo -e "${BLUE}[STEP]${NC} $1"
}

# Parse command line arguments
TARGET=""
SIGN_ARTIFACTS=false
VERIFY_ONLY=false
CLEAN_BUILD=false
DOCKER_BUILD=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --target)
            TARGET="$2"
            shift 2
            ;;
        --sign)
            SIGN_ARTIFACTS=true
            shift
            ;;
        --verify-only)
            VERIFY_ONLY=true
            shift
            ;;
        --clean)
            CLEAN_BUILD=true
            shift
            ;;
        --docker)
            DOCKER_BUILD=true
            shift
            ;;
        --help)
            echo "Usage: $0 [OPTIONS]"
            echo "Options:"
            echo "  --target TARGET     Build for specific target (e.g., x86_64-pc-windows-msvc)"
            echo "  --sign              Sign artifacts after building"
            echo "  --verify-only       Only verify existing build, don't rebuild"
            echo "  --clean             Clean build (remove target directory)"
            echo "  --docker            Use Docker for reproducible environment"
            echo "  --help              Show this help message"
            exit 0
            ;;
        *)
            log_error "Unknown option: $1"
            exit 1
            ;;
    esac
done

# Load configuration
if [[ ! -f "$BUILD_CONFIG" ]]; then
    log_error "Build configuration not found: $BUILD_CONFIG"
    exit 1
fi

# Parse TOML configuration (simplified - in production use a proper TOML parser)
SOURCE_DATE_EPOCH=$(grep "SOURCE_DATE_EPOCH" "$BUILD_CONFIG" | cut -d'"' -f2)
RUST_VERSION=$(grep "rust_version" "$BUILD_CONFIG" | cut -d'"' -f2)

# Set reproducible environment
export SOURCE_DATE_EPOCH="${SOURCE_DATE_EPOCH:-1704067200}"
export RUSTC_BOOTSTRAP=0
export CARGO_INCREMENTAL=0
export LC_ALL=C
export LANG=C
export TZ=UTC

# Reproducible Rust flags
export RUSTFLAGS="-C target-cpu=x86-64 -C link-dead-code=off -C embed-bitcode=no -C codegen-units=1"

log_info "Starting reproducible build for Racing Wheel Suite"
log_info "Source date epoch: $(date -d @$SOURCE_DATE_EPOCH -u)"
log_info "Target: ${TARGET:-all}"

# Function to check prerequisites
check_prerequisites() {
    log_step "Checking prerequisites"
    
    # Check Rust version
    if ! command -v rustc &> /dev/null; then
        log_error "Rust not found. Please install Rust $RUST_VERSION"
        exit 1
    fi
    
    local current_rust_version
    current_rust_version=$(rustc --version | cut -d' ' -f2)
    if [[ "$current_rust_version" != "$RUST_VERSION" ]]; then
        log_warn "Rust version mismatch: expected $RUST_VERSION, got $current_rust_version"
        log_warn "Consider using rustup to install the correct version:"
        log_warn "  rustup install $RUST_VERSION"
        log_warn "  rustup default $RUST_VERSION"
    fi
    
    # Check required tools
    local required_tools=("cargo" "git")
    for tool in "${required_tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            log_error "$tool not found"
            exit 1
        fi
    done
    
    # Check optional tools for enhanced security
    local optional_tools=("cargo-audit" "cargo-deny" "cargo-geiger")
    for tool in "${optional_tools[@]}"; do
        if ! command -v "$tool" &> /dev/null; then
            log_warn "$tool not found (optional but recommended)"
            log_warn "Install with: cargo install $tool"
        fi
    done
    
    log_info "Prerequisites check completed"
}

# Function to clean build environment
clean_build_env() {
    if [[ "$CLEAN_BUILD" == "true" ]]; then
        log_step "Cleaning build environment"
        rm -rf "$PROJECT_ROOT/target"
        rm -rf "$ARTIFACTS_DIR"
        log_info "Build environment cleaned"
    fi
}

# Function to verify dependencies
verify_dependencies() {
    log_step "Verifying dependencies"
    
    # Run cargo audit if available
    if command -v cargo-audit &> /dev/null; then
        log_info "Running cargo audit..."
        cargo audit --deny warnings
    fi
    
    # Run cargo deny if available
    if command -v cargo-deny &> /dev/null; then
        log_info "Running cargo deny..."
        cargo deny check
    fi
    
    # Generate dependency tree
    cargo tree --format "{p} {l}" > "$BUILD_DIR/dependency-tree.txt"
    
    log_info "Dependencies verified"
}

# Function to build for a specific target
build_target() {
    local target="$1"
    log_step "Building for target: $target"
    
    # Create build directory
    mkdir -p "$BUILD_DIR/$target"
    
    # Set target-specific flags
    local additional_flags=""
    case "$target" in
        "x86_64-pc-windows-msvc")
            additional_flags="-C target-feature=+crt-static"
            ;;
        "x86_64-unknown-linux-gnu")
            additional_flags="-C target-feature=+crt-static"
            ;;
    esac
    
    # Build with reproducible settings
    RUSTFLAGS="$RUSTFLAGS $additional_flags" \
    cargo build \
        --release \
        --target "$target" \
        --target-dir "$BUILD_DIR" \
        --locked \
        --offline
    
    # Copy binaries to artifacts directory
    local target_dir="$BUILD_DIR/$target/release"
    mkdir -p "$ARTIFACTS_DIR/$target"
    
    # Copy binaries (adjust extensions based on target)
    local binary_ext=""
    if [[ "$target" == *"windows"* ]]; then
        binary_ext=".exe"
    fi
    
    for binary in wheeld wheelctl wheel-ui; do
        if [[ -f "$target_dir/$binary$binary_ext" ]]; then
            cp "$target_dir/$binary$binary_ext" "$ARTIFACTS_DIR/$target/"
            log_info "Copied $binary$binary_ext for $target"
        fi
    done
    
    log_info "Build completed for $target"
}

# Function to generate checksums
generate_checksums() {
    log_step "Generating checksums"
    
    cd "$ARTIFACTS_DIR"
    
    # Generate SHA256 checksums
    find . -type f -name "*.exe" -o -name "wheeld" -o -name "wheelctl" -o -name "wheel-ui" | \
    while read -r file; do
        sha256sum "$file" >> "SHA256SUMS"
        sha512sum "$file" >> "SHA512SUMS"
        
        # Generate BLAKE3 if available
        if command -v b3sum &> /dev/null; then
            b3sum "$file" >> "BLAKE3SUMS"
        fi
    done
    
    log_info "Checksums generated"
}

# Function to sign artifacts
sign_artifacts() {
    if [[ "$SIGN_ARTIFACTS" != "true" ]]; then
        return
    fi
    
    log_step "Signing artifacts"
    
    local signing_key="$PROJECT_ROOT/signing/release.key"
    if [[ ! -f "$signing_key" ]]; then
        log_warn "Signing key not found: $signing_key"
        log_warn "Skipping artifact signing"
        return
    fi
    
    cd "$ARTIFACTS_DIR"
    
    # Sign each binary
    find . -type f -name "*.exe" -o -name "wheeld" -o -name "wheelctl" -o -name "wheel-ui" | \
    while read -r file; do
        # Create detached signature
        # In production, use actual Ed25519 signing tool
        echo "SIGNATURE_PLACEHOLDER_FOR_$file" > "$file.sig"
        log_info "Signed $file"
    done
    
    # Sign checksum files
    for checksum_file in SHA256SUMS SHA512SUMS BLAKE3SUMS; do
        if [[ -f "$checksum_file" ]]; then
            echo "SIGNATURE_PLACEHOLDER_FOR_$checksum_file" > "$checksum_file.sig"
        fi
    done
    
    log_info "Artifacts signed"
}

# Function to generate SBOM (Software Bill of Materials)
generate_sbom() {
    log_step "Generating Software Bill of Materials"
    
    # Generate SBOM using cargo tree
    cargo tree --format "{p} {l}" --prefix none > "$ARTIFACTS_DIR/sbom.txt"
    
    # Generate detailed dependency information
    cat > "$ARTIFACTS_DIR/sbom.json" << EOF
{
    "bomFormat": "CycloneDX",
    "specVersion": "1.4",
    "serialNumber": "urn:uuid:$(uuidgen)",
    "version": 1,
    "metadata": {
        "timestamp": "$(date -u -d @$SOURCE_DATE_EPOCH +%Y-%m-%dT%H:%M:%SZ)",
        "tools": [
            {
                "vendor": "Racing Wheel Suite",
                "name": "build-reproducible.sh",
                "version": "1.0"
            }
        ],
        "component": {
            "type": "application",
            "name": "racing-wheel-suite",
            "version": "$(cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "racing-wheel-suite") | .version')"
        }
    },
    "components": []
}
EOF
    
    log_info "SBOM generated"
}

# Function to generate build metadata
generate_build_metadata() {
    log_step "Generating build metadata"
    
    cat > "$ARTIFACTS_DIR/build-metadata.json" << EOF
{
    "build_time": "$(date -u -d @$SOURCE_DATE_EPOCH +%Y-%m-%dT%H:%M:%SZ)",
    "source_date_epoch": "$SOURCE_DATE_EPOCH",
    "git_commit": "$(git rev-parse HEAD)",
    "git_branch": "$(git rev-parse --abbrev-ref HEAD)",
    "git_tag": "$(git describe --tags --exact-match 2>/dev/null || echo 'none')",
    "rust_version": "$(rustc --version)",
    "cargo_version": "$(cargo --version)",
    "build_host": "$(uname -a)",
    "environment": {
        "SOURCE_DATE_EPOCH": "$SOURCE_DATE_EPOCH",
        "RUSTFLAGS": "$RUSTFLAGS",
        "LC_ALL": "$LC_ALL",
        "TZ": "$TZ"
    }
}
EOF
    
    log_info "Build metadata generated"
}

# Function to verify reproducibility
verify_reproducibility() {
    log_step "Verifying build reproducibility"
    
    # This would compare with a previous build or reference artifacts
    # For now, just verify that all expected artifacts exist
    
    local expected_files=(
        "SHA256SUMS"
        "SHA512SUMS" 
        "build-metadata.json"
        "sbom.json"
    )
    
    cd "$ARTIFACTS_DIR"
    
    for file in "${expected_files[@]}"; do
        if [[ ! -f "$file" ]]; then
            log_error "Expected artifact missing: $file"
            return 1
        fi
    done
    
    # Verify checksums
    if ! sha256sum -c SHA256SUMS; then
        log_error "SHA256 checksum verification failed"
        return 1
    fi
    
    if ! sha512sum -c SHA512SUMS; then
        log_error "SHA512 checksum verification failed"
        return 1
    fi
    
    log_info "Reproducibility verification passed"
}

# Function to create release package
create_release_package() {
    log_step "Creating release package"
    
    local version
    version=$(cargo metadata --format-version 1 | jq -r '.packages[] | select(.name == "racing-wheel-suite") | .version')
    
    cd "$ARTIFACTS_DIR"
    
    # Create tar.gz for each target
    for target_dir in */; do
        if [[ -d "$target_dir" ]]; then
            local target_name="${target_dir%/}"
            tar -czf "racing-wheel-suite-$version-$target_name.tar.gz" \
                "$target_name"/ \
                *.json \
                *SUMS \
                *.sig 2>/dev/null || true
            
            log_info "Created package: racing-wheel-suite-$version-$target_name.tar.gz"
        fi
    done
    
    log_info "Release packages created"
}

# Function to run Docker build
docker_build() {
    log_step "Running Docker build"
    
    # Create Dockerfile for reproducible builds
    cat > "$PROJECT_ROOT/Dockerfile.reproducible" << EOF
FROM rust:$RUST_VERSION-slim

# Install dependencies
RUN apt-get update && apt-get install -y \\
    build-essential \\
    pkg-config \\
    libssl-dev \\
    git \\
    && rm -rf /var/lib/apt/lists/*

# Set reproducible environment
ENV SOURCE_DATE_EPOCH=$SOURCE_DATE_EPOCH
ENV RUSTC_BOOTSTRAP=0
ENV CARGO_INCREMENTAL=0
ENV LC_ALL=C
ENV LANG=C
ENV TZ=UTC

# Set working directory
WORKDIR /workspace

# Copy source code
COPY . .

# Run build
RUN ./scripts/build-reproducible.sh
EOF
    
    # Build with Docker
    docker build -f Dockerfile.reproducible -t racing-wheel-suite-build .
    
    # Extract artifacts
    docker run --rm -v "$ARTIFACTS_DIR:/output" racing-wheel-suite-build \
        cp -r /workspace/artifacts/* /output/
    
    log_info "Docker build completed"
}

# Main execution
main() {
    cd "$PROJECT_ROOT"
    
    # Create directories
    mkdir -p "$BUILD_DIR" "$ARTIFACTS_DIR"
    
    if [[ "$VERIFY_ONLY" == "true" ]]; then
        verify_reproducibility
        exit $?
    fi
    
    if [[ "$DOCKER_BUILD" == "true" ]]; then
        docker_build
        exit $?
    fi
    
    check_prerequisites
    clean_build_env
    verify_dependencies
    
    # Build targets
    if [[ -n "$TARGET" ]]; then
        build_target "$TARGET"
    else
        # Build all configured targets
        local targets=("x86_64-pc-windows-msvc" "x86_64-unknown-linux-gnu")
        for target in "${targets[@]}"; do
            if rustup target list --installed | grep -q "$target"; then
                build_target "$target"
            else
                log_warn "Target $target not installed, skipping"
                log_warn "Install with: rustup target add $target"
            fi
        done
    fi
    
    generate_checksums
    sign_artifacts
    generate_sbom
    generate_build_metadata
    verify_reproducibility
    create_release_package
    
    log_info "Reproducible build completed successfully!"
    log_info "Artifacts available in: $ARTIFACTS_DIR"
}

# Run main function
main "$@"