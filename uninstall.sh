#!/bin/bash
# argo-rs uninstaller
# Usage: curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/uninstall.sh | bash

set -e

BINARY_NAME="argo"
INSTALL_DIR="${HOME}/.local/bin"

# Config paths
MACOS_CONFIG_DIR="${HOME}/Library/Application Support/com.argo-rs.argo-rs"
LINUX_CONFIG_DIR="${HOME}/.config/argo-rs"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

prompt() {
    echo -e "${BLUE}[?]${NC} $1"
}

# Detect OS
detect_os() {
    local os
    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    case "$os" in
        darwin) echo "macos" ;;
        linux) echo "linux" ;;
        *) echo "unknown" ;;
    esac
}

# Remove binary
remove_binary() {
    if [ -f "$INSTALL_DIR/$BINARY_NAME" ]; then
        rm -f "$INSTALL_DIR/$BINARY_NAME"
        info "Removed $INSTALL_DIR/$BINARY_NAME"
    else
        warn "Binary not found in $INSTALL_DIR"
    fi

    # Also remove old 'gr' binary if it exists (from previous versions)
    if [ -f "$INSTALL_DIR/gr" ]; then
        rm -f "$INSTALL_DIR/gr"
        info "Removed old $INSTALL_DIR/gr binary"
    fi
}

# Remove config files
remove_config() {
    local os config_dir
    os=$(detect_os)

    case "$os" in
        macos) config_dir="$MACOS_CONFIG_DIR" ;;
        linux) config_dir="$LINUX_CONFIG_DIR" ;;
        *)
            warn "Unknown OS, skipping config removal"
            return
            ;;
    esac

    if [ -d "$config_dir" ]; then
        prompt "Remove configuration directory? ($config_dir) [y/N] "
        read -r response
        case "$response" in
            [yY][eE][sS]|[yY])
                rm -rf "$config_dir"
                info "Removed configuration directory"
                ;;
            *)
                info "Keeping configuration directory"
                ;;
        esac
    else
        info "No configuration directory found"
    fi
}

# Show credentials removal instructions
show_credentials_info() {
    local os
    os=$(detect_os)

    echo ""
    warn "Stored credentials must be removed manually for security reasons."
    echo ""

    case "$os" in
        macos)
            echo "To remove stored credentials from macOS Keychain:"
            echo "  1. Open 'Keychain Access' app"
            echo "  2. Search for 'argo-rs'"
            echo "  3. Delete any matching entries"
            echo ""
            echo "Or use the command line:"
            echo "  security delete-generic-password -s 'argo-rs' 2>/dev/null || true"
            ;;
        linux)
            echo "To remove stored credentials from Secret Service:"
            echo "  1. Open your system's password manager (e.g., GNOME Keyring, KWallet)"
            echo "  2. Search for 'argo-rs'"
            echo "  3. Delete any matching entries"
            echo ""
            echo "Or use secret-tool if available:"
            echo "  secret-tool clear service argo-rs"
            ;;
        *)
            echo "Check your system's credential manager for 'argo-rs' entries."
            ;;
    esac
}

# Main uninstall function
uninstall() {
    echo ""
    echo "argo-rs Uninstaller"
    echo "==================="
    echo ""

    info "Removing binary..."
    remove_binary

    echo ""
    remove_config

    show_credentials_info

    echo ""
    info "Uninstall complete!"
    echo ""
}

# Run uninstaller
uninstall
