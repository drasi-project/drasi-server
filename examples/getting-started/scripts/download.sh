#!/bin/bash
# Drasi Server Install Script
# Auto-detects platform and downloads the correct binaries

set -e

REPO_URL="https://github.com/drasi-project/drasi-server/releases/latest/download"
INSTALL_DIR="bin"

# Detect OS
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Darwin)
        case "$ARCH" in
            arm64)
                PLATFORM_SUFFIX="aarch64-apple-darwin"
                ;;
            x86_64)
                PLATFORM_SUFFIX="x86_64-apple-darwin"
                ;;
            *)
                echo "Error: Unsupported macOS architecture: $ARCH"
                exit 1
                ;;
        esac
        ;;
    Linux)
        # Check for musl libc (Alpine, etc.)
        if ldd --version 2>&1 | grep -q musl; then
            LIBC="musl"
        else
            LIBC="gnu"
        fi
        
        case "$ARCH" in
            x86_64)
                PLATFORM_SUFFIX="x86_64-linux-$LIBC"
                ;;
            aarch64|arm64)
                PLATFORM_SUFFIX="aarch64-linux-$LIBC"
                ;;
            *)
                echo "Error: Unsupported Linux architecture: $ARCH"
                exit 1
                ;;
        esac
        
        # musl systems may need additional libraries
        if [ "$LIBC" = "musl" ]; then
            echo "Note: musl libc detected. Ensure libstdc++ and libgcc are installed:"
            echo "  apk add --no-cache libstdc++ libgcc"
        fi
        ;;
    *)
        echo "Error: Unsupported operating system: $OS"
        echo "For Windows, use install.ps1 instead."
        exit 1
        ;;
esac

echo "Detected: $OS ($ARCH)"

# Create install directory
mkdir -p "$INSTALL_DIR"

# Download drasi-server
SERVER_BINARY="drasi-server-$PLATFORM_SUFFIX"
echo "Downloading: $SERVER_BINARY"
curl -fsSL "$REPO_URL/$SERVER_BINARY" -o "$INSTALL_DIR/drasi-server"
chmod +x "$INSTALL_DIR/drasi-server"

# Download drasi-sse-cli
SSE_BINARY="drasi-sse-cli-$PLATFORM_SUFFIX"
echo "Downloading: $SSE_BINARY"
curl -fsSL "$REPO_URL/$SSE_BINARY" -o "$INSTALL_DIR/drasi-sse-cli"
chmod +x "$INSTALL_DIR/drasi-sse-cli"

# Verify
echo ""
echo "Verifying installations..."
"$INSTALL_DIR/drasi-server" --version
"$INSTALL_DIR/drasi-sse-cli" --version

echo ""
echo "✅ Drasi Server installed to $INSTALL_DIR/drasi-server"
echo "✅ Drasi SSE CLI installed to $INSTALL_DIR/drasi-sse-cli"
