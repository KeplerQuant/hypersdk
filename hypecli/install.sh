#!/bin/sh
# hypecli installer
# Usage: curl -fsSL https://raw.githubusercontent.com/infinitefield/hypersdk/main/hypecli/install.sh | sh

set -e

REPO="infinitefield/hypersdk"
BINARY="hypecli"
INSTALL_DIR="${INSTALL_DIR:-/usr/local/bin}"

# Detect OS and architecture
detect_platform() {
    OS="$(uname -s)"
    ARCH="$(uname -m)"

    case "$OS" in
        Linux)
            OS="linux"
            ;;
        Darwin)
            OS="darwin"
            ;;
        *)
            echo "Error: Unsupported operating system: $OS"
            exit 1
            ;;
    esac

    case "$ARCH" in
        x86_64|amd64)
            ARCH="x86_64"
            ;;
        arm64|aarch64)
            ARCH="aarch64"
            ;;
        *)
            echo "Error: Unsupported architecture: $ARCH"
            exit 1
            ;;
    esac

    PLATFORM="${OS}-${ARCH}"
}

# Get the latest release version
get_latest_version() {
    VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/')
    if [ -z "$VERSION" ]; then
        echo "Error: Could not determine latest version"
        exit 1
    fi
}

# Download and install
install() {
    detect_platform
    get_latest_version

    echo "Installing ${BINARY} ${VERSION} for ${PLATFORM}..."

    # Construct download URL
    # Adjust this pattern based on your actual release asset naming
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${VERSION}/${BINARY}-${PLATFORM}"

    # Create temp directory
    TMP_DIR=$(mktemp -d)
    trap 'rm -rf "$TMP_DIR"' EXIT

    # Download binary
    echo "Downloading from ${DOWNLOAD_URL}..."
    if ! curl -fsSL "$DOWNLOAD_URL" -o "${TMP_DIR}/${BINARY}"; then
        echo "Error: Failed to download ${BINARY}"
        echo ""
        echo "Release assets may not be available yet."
        echo "You can build from source instead:"
        echo "  cargo install --git https://github.com/${REPO} --bin ${BINARY}"
        exit 1
    fi

    # Make executable
    chmod +x "${TMP_DIR}/${BINARY}"

    # Install
    if [ -w "$INSTALL_DIR" ]; then
        mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
    else
        echo "Installing to ${INSTALL_DIR} requires sudo..."
        sudo mv "${TMP_DIR}/${BINARY}" "${INSTALL_DIR}/${BINARY}"
    fi

    echo ""
    echo "Successfully installed ${BINARY} to ${INSTALL_DIR}/${BINARY}"
    echo ""
    echo "Run '${BINARY} --help' to get started"
    echo "Run '${BINARY} --agent-help' for detailed AI agent documentation"
}

# Alternative: install via cargo
install_cargo() {
    echo "Installing ${BINARY} via cargo..."
    cargo install --git "https://github.com/${REPO}" --bin "${BINARY}"
    echo ""
    echo "Successfully installed ${BINARY}"
    echo ""
    echo "Run '${BINARY} --help' to get started"
    echo "Run '${BINARY} --agent-help' for detailed AI agent documentation"
}

# Main
main() {
    echo "hypecli installer"
    echo "================="
    echo ""

    # Check if cargo is preferred or if we should try binary first
    if [ "$1" = "--cargo" ] || [ "$USE_CARGO" = "1" ]; then
        if command -v cargo >/dev/null 2>&1; then
            install_cargo
        else
            echo "Error: cargo not found. Please install Rust first:"
            echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
            exit 1
        fi
    else
        # Try binary install, fall back to cargo
        install
    fi
}

main "$@"
