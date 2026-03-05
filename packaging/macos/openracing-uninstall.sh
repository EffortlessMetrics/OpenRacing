#!/bin/bash
# OpenRacing macOS Uninstaller
#
# Removes the OpenRacing Racing Wheel Suite from macOS.
#
# Usage:
#   ./openracing-uninstall.sh [--keep-config]

set -euo pipefail

APP_BUNDLE="/Applications/OpenRacing.app"
LAUNCH_DAEMON="/Library/LaunchDaemons/com.openracing.wheeld.plist"
LAUNCH_AGENT="$HOME/Library/LaunchAgents/com.openracing.wheeld.plist"
CONFIG_DIR="$HOME/Library/Application Support/OpenRacing"
CACHE_DIR="$HOME/Library/Caches/OpenRacing"
LOG_DIR="$HOME/Library/Logs/OpenRacing"
PREFS_FILE="$HOME/Library/Preferences/com.openracing.wheel-suite.plist"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

log_info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
log_warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_error() { echo -e "${RED}[ERROR]${NC} $1"; }

KEEP_CONFIG=false

for arg in "$@"; do
    case "$arg" in
        --keep-config) KEEP_CONFIG=true ;;
        -h|--help)
            echo "Usage: $(basename "$0") [--keep-config]"
            echo ""
            echo "Uninstalls OpenRacing Racing Wheel Suite from macOS."
            echo ""
            echo "Options:"
            echo "  --keep-config   Preserve user configuration and profiles"
            echo "  -h, --help      Show this help message"
            exit 0
            ;;
        *)
            log_error "Unknown option: $arg"
            exit 1
            ;;
    esac
done

log_info "OpenRacing Uninstaller"
echo ""

# Stop the service if running
if launchctl list 2>/dev/null | grep -q "com.openracing.wheeld"; then
    log_info "Stopping OpenRacing service..."
    launchctl bootout system/com.openracing.wheeld 2>/dev/null || \
        launchctl bootout "gui/$(id -u)/com.openracing.wheeld" 2>/dev/null || \
        true
fi

# Remove launch daemon (requires sudo)
if [ -f "$LAUNCH_DAEMON" ]; then
    log_info "Removing launch daemon..."
    sudo rm -f "$LAUNCH_DAEMON"
fi

# Remove launch agent
if [ -f "$LAUNCH_AGENT" ]; then
    log_info "Removing launch agent..."
    rm -f "$LAUNCH_AGENT"
fi

# Remove application bundle
if [ -d "$APP_BUNDLE" ]; then
    log_info "Removing application bundle..."
    sudo rm -rf "$APP_BUNDLE"
fi

# Remove caches and logs (always removed)
if [ -d "$CACHE_DIR" ]; then
    log_info "Removing cache directory..."
    rm -rf "$CACHE_DIR"
fi

if [ -d "$LOG_DIR" ]; then
    log_info "Removing log directory..."
    rm -rf "$LOG_DIR"
fi

# Remove preferences plist
if [ -f "$PREFS_FILE" ]; then
    log_info "Removing preferences..."
    rm -f "$PREFS_FILE"
fi

# Conditionally remove configuration
if [ "$KEEP_CONFIG" = false ]; then
    if [ -d "$CONFIG_DIR" ]; then
        log_info "Removing configuration directory..."
        rm -rf "$CONFIG_DIR"
    fi
else
    log_warn "Keeping configuration at: $CONFIG_DIR"
fi

echo ""
log_info "OpenRacing has been uninstalled."
