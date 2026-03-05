#!/bin/bash
# OpenRacing macOS DMG Build Script
#
# Creates a signed DMG disk image containing the OpenRacing.app bundle.
#
# Requirements: 19.2 - macOS DMG with signed app bundle
#
# Usage:
#   ./create-dmg.sh --bin-path <path> [--version <version>] [--output <dir>]
#                    [--sign-identity <identity>] [--notarize]
#
# Dependencies:
#   - hdiutil (macOS built-in)
#   - codesign (macOS built-in)
#   - xcrun notarytool (for notarization, optional)

set -euo pipefail

# Script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

# Default values
BIN_PATH=""
OUTPUT_DIR="dist"
VERSION=""
SIGN_IDENTITY=""
NOTARIZE=false
VOLUME_NAME="OpenRacing"
DMG_FILENAME=""
APP_BUNDLE_NAME="OpenRacing.app"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }
log_step()  { echo -e "${BLUE}[STEP]${NC} $1"; }

usage() {
    cat << EOF
Usage: $(basename "$0") [OPTIONS]

Build macOS DMG disk image for OpenRacing.

Options:
    --bin-path <path>          Path to compiled binaries (required)
    --version <version>        Package version (default: from Cargo.toml)
    --output <dir>             Output directory (default: dist)
    --sign-identity <id>       Code signing identity for codesign
    --notarize                 Submit to Apple notarization service
    -h, --help                 Show this help message
EOF
}

# Parse arguments
while [[ $# -gt 0 ]]; do
    case "$1" in
        --bin-path)     BIN_PATH="$2"; shift 2 ;;
        --version)      VERSION="$2"; shift 2 ;;
        --output)       OUTPUT_DIR="$2"; shift 2 ;;
        --sign-identity) SIGN_IDENTITY="$2"; shift 2 ;;
        --notarize)     NOTARIZE=true; shift ;;
        -h|--help)      usage; exit 0 ;;
        *)              log_error "Unknown option: $1"; usage; exit 1 ;;
    esac
done

# Validate required arguments
if [ -z "$BIN_PATH" ]; then
    log_error "--bin-path is required"
    usage
    exit 1
fi

if [ ! -d "$BIN_PATH" ]; then
    log_error "Binary path does not exist: $BIN_PATH"
    exit 1
fi

# Auto-detect version from Cargo.toml if not provided
if [ -z "$VERSION" ]; then
    VERSION=$(grep -m1 '^version' "$PROJECT_ROOT/Cargo.toml" | sed 's/.*"\(.*\)".*/\1/')
    log_info "Auto-detected version: $VERSION"
fi

DMG_FILENAME="OpenRacing-${VERSION}-macOS.dmg"

# Required binaries
REQUIRED_BINS=("wheeld" "wheelctl")
OPTIONAL_BINS=("openracing")

log_step "Validating binaries..."
for bin in "${REQUIRED_BINS[@]}"; do
    if [ ! -f "$BIN_PATH/$bin" ]; then
        log_error "Required binary not found: $BIN_PATH/$bin"
        exit 1
    fi
    log_info "Found required binary: $bin ($(du -h "$BIN_PATH/$bin" | cut -f1))"
done

for bin in "${OPTIONAL_BINS[@]}"; do
    if [ -f "$BIN_PATH/$bin" ]; then
        log_info "Found optional binary: $bin"
    else
        log_warn "Optional binary not found: $bin (skipping)"
    fi
done

# Create output directory
mkdir -p "$OUTPUT_DIR"

# Create temporary build area
BUILD_DIR=$(mktemp -d)
trap 'rm -rf "$BUILD_DIR"' EXIT

APP_DIR="$BUILD_DIR/$APP_BUNDLE_NAME"

log_step "Creating app bundle structure..."
mkdir -p "$APP_DIR/Contents/MacOS"
mkdir -p "$APP_DIR/Contents/Resources"
mkdir -p "$APP_DIR/Contents/Resources/config"
mkdir -p "$APP_DIR/Contents/Resources/docs"

# Copy Info.plist with version substitution
log_step "Generating Info.plist..."
sed "s/0\.1\.0/$VERSION/g" "$SCRIPT_DIR/Info.plist" > "$APP_DIR/Contents/Info.plist"

# Copy binaries
log_step "Copying binaries..."
for bin in "${REQUIRED_BINS[@]}"; do
    cp "$BIN_PATH/$bin" "$APP_DIR/Contents/MacOS/"
    chmod 755 "$APP_DIR/Contents/MacOS/$bin"
done

for bin in "${OPTIONAL_BINS[@]}"; do
    if [ -f "$BIN_PATH/$bin" ]; then
        cp "$BIN_PATH/$bin" "$APP_DIR/Contents/MacOS/"
        chmod 755 "$APP_DIR/Contents/MacOS/$bin"
    fi
done

# Copy resources
log_step "Copying resources..."
if [ -f "$PROJECT_ROOT/README.md" ]; then
    cp "$PROJECT_ROOT/README.md" "$APP_DIR/Contents/Resources/docs/"
fi
if [ -f "$PROJECT_ROOT/LICENSE-MIT" ]; then
    cp "$PROJECT_ROOT/LICENSE-MIT" "$APP_DIR/Contents/Resources/docs/"
fi
if [ -f "$PROJECT_ROOT/LICENSE-APACHE" ]; then
    cp "$PROJECT_ROOT/LICENSE-APACHE" "$APP_DIR/Contents/Resources/docs/"
fi

# Copy uninstaller
cp "$SCRIPT_DIR/openracing-uninstall.sh" "$APP_DIR/Contents/Resources/"
chmod 755 "$APP_DIR/Contents/Resources/openracing-uninstall.sh"

# Code signing
if [ -n "$SIGN_IDENTITY" ]; then
    log_step "Code signing app bundle..."
    codesign --force --deep --sign "$SIGN_IDENTITY" \
        --entitlements "$SCRIPT_DIR/entitlements.plist" \
        --options runtime \
        --timestamp \
        "$APP_DIR"
    log_info "Code signing complete"

    # Verify signature
    codesign --verify --deep --strict "$APP_DIR"
    log_info "Signature verification passed"
else
    log_warn "No signing identity provided — DMG will be unsigned"
fi

# Create DMG
log_step "Creating DMG image..."
DMG_PATH="$OUTPUT_DIR/$DMG_FILENAME"

# Create a temporary DMG with read-write access
TEMP_DMG="$BUILD_DIR/temp.dmg"
hdiutil create -size 200m -fs HFS+ -volname "$VOLUME_NAME" "$TEMP_DMG"
MOUNT_POINT=$(hdiutil attach "$TEMP_DMG" | grep "/Volumes" | awk '{print $NF}')

# Copy app bundle into DMG
cp -R "$APP_DIR" "$MOUNT_POINT/"

# Create Applications symlink for drag-and-drop install
ln -s /Applications "$MOUNT_POINT/Applications"

# Unmount and convert to compressed read-only DMG
hdiutil detach "$MOUNT_POINT"
hdiutil convert "$TEMP_DMG" -format UDZO -o "$DMG_PATH"

log_info "DMG created: $DMG_PATH"

# Sign the DMG itself if identity provided
if [ -n "$SIGN_IDENTITY" ]; then
    log_step "Signing DMG..."
    codesign --force --sign "$SIGN_IDENTITY" --timestamp "$DMG_PATH"
    log_info "DMG signed"
fi

# Notarization
if [ "$NOTARIZE" = true ] && [ -n "$SIGN_IDENTITY" ]; then
    log_step "Submitting for Apple notarization..."
    xcrun notarytool submit "$DMG_PATH" --wait
    xcrun stapler staple "$DMG_PATH"
    log_info "Notarization complete"
elif [ "$NOTARIZE" = true ]; then
    log_warn "Notarization requires a signing identity — skipping"
fi

# Generate checksums
log_step "Generating checksums..."
shasum -a 256 "$DMG_PATH" > "$DMG_PATH.sha256"
log_info "SHA-256: $(cat "$DMG_PATH.sha256")"

# Generate build metadata
cat > "$OUTPUT_DIR/build-metadata.json" << METADATA
{
    "product": "OpenRacing",
    "version": "$VERSION",
    "platform": "macOS",
    "minimum_os": "10.15",
    "format": "dmg",
    "filename": "$DMG_FILENAME",
    "signed": $([ -n "$SIGN_IDENTITY" ] && echo "true" || echo "false"),
    "notarized": $NOTARIZE,
    "build_date": "$(date -u +%Y-%m-%dT%H:%M:%SZ)",
    "binaries": {
        "required": ["wheeld", "wheelctl"],
        "optional": ["openracing"]
    }
}
METADATA

log_info "Build metadata written to $OUTPUT_DIR/build-metadata.json"

echo ""
log_info "macOS DMG build complete!"
log_info "  DMG: $DMG_PATH"
log_info "  Version: $VERSION"
