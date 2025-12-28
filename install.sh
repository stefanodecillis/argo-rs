#!/bin/bash
# argo-rs installer
# Usage: curl -sSL https://raw.githubusercontent.com/stefanodecillis/argo-rs/main/install.sh | bash

set -e

REPO="stefanodecillis/argo-rs"
BINARY_NAME="gr"
BINARY_ALIAS="argo"
INSTALL_DIR="${HOME}/.local/bin"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
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

# Detect OS and architecture
detect_platform() {
    local os arch

    os=$(uname -s | tr '[:upper:]' '[:lower:]')
    arch=$(uname -m)

    case "$os" in
        darwin)
            os="macos"
            ;;
        linux)
            os="linux"
            ;;
        *)
            error "Unsupported operating system: $os"
            ;;
    esac

    case "$arch" in
        x86_64|amd64)
            arch="x86_64"
            ;;
        arm64|aarch64)
            arch="aarch64"
            ;;
        *)
            error "Unsupported architecture: $arch"
            ;;
    esac

    echo "${os}-${arch}"
}

# Get latest release version
get_latest_version() {
    curl -sSL "https://api.github.com/repos/${REPO}/releases/latest" | \
        grep '"tag_name":' | \
        sed -E 's/.*"([^"]+)".*/\1/'
}

# Download and install
install() {
    local platform version download_url temp_dir

    info "Detecting platform..."
    platform=$(detect_platform)
    info "Platform: $platform"

    info "Fetching latest version..."
    version=$(get_latest_version)
    if [ -z "$version" ]; then
        error "Could not determine latest version. Please check https://github.com/${REPO}/releases"
    fi
    info "Latest version: $version"

    download_url="https://github.com/${REPO}/releases/download/${version}/gr-${platform}.tar.gz"
    info "Download URL: $download_url"

    # Create temp directory
    temp_dir=$(mktemp -d)
    trap "rm -rf $temp_dir" EXIT

    # Download
    info "Downloading..."
    if ! curl -sSL "$download_url" -o "$temp_dir/gr.tar.gz"; then
        error "Failed to download from $download_url"
    fi

    # Extract
    info "Extracting..."
    tar -xzf "$temp_dir/gr.tar.gz" -C "$temp_dir"

    # Create install directory if needed
    mkdir -p "$INSTALL_DIR"

    # Install both binaries
    info "Installing to $INSTALL_DIR..."
    mv "$temp_dir/$BINARY_NAME" "$INSTALL_DIR/"
    chmod +x "$INSTALL_DIR/$BINARY_NAME"

    # Create argo alias (symlink to gr)
    ln -sf "$INSTALL_DIR/$BINARY_NAME" "$INSTALL_DIR/$BINARY_ALIAS"

    # Sign binary on macOS for Keychain "Always Allow" to persist
    if [ "$(uname -s)" = "Darwin" ]; then
        info "Signing binary for macOS Keychain compatibility..."
        if codesign --force --deep --sign - "$INSTALL_DIR/$BINARY_NAME" 2>/dev/null; then
            info "Binary signed successfully"
        else
            warn "Could not sign binary (Keychain 'Always Allow' may not persist across restarts)"
        fi
    fi

    info "Successfully installed $BINARY_NAME and $BINARY_ALIAS to $INSTALL_DIR"

    # Check if in PATH
    if ! echo "$PATH" | grep -q "$INSTALL_DIR"; then
        warn "$INSTALL_DIR is not in your PATH"
        echo ""
        echo "Add it to your shell configuration:"
        echo ""
        echo "  For bash (~/.bashrc):"
        echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        echo "  For zsh (~/.zshrc):"
        echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        echo "Then restart your shell or run: source ~/.bashrc (or ~/.zshrc)"
    fi

    echo ""
    info "Installation complete!"
    echo ""
    echo "Get started:"
    echo "  1. Authenticate: $BINARY_NAME auth login"
    echo "  2. Navigate to a git repository"
    echo "  3. Run: $BINARY_NAME (or $BINARY_ALIAS)"
    echo ""
    echo "For help: $BINARY_NAME --help"
}

# Run installer
install
