#!/usr/bin/env bash
set -euo pipefail

# crsllm installer — builds release binary and installs to PATH
# Usage: curl -fsSL ... | bash  OR  ./scripts/install.sh

BINARY="crsllm"
INSTALL_DIR="${CRSLLM_INSTALL_DIR:-$HOME/.cargo/bin}"
REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"

echo "🔧 Building crsllm (release)..."
cd "$REPO_ROOT"
cargo build --release --quiet

echo "📦 Installing to $INSTALL_DIR/$BINARY"
mkdir -p "$INSTALL_DIR"
cp "target/release/$BINARY" "$INSTALL_DIR/$BINARY"
chmod +x "$INSTALL_DIR/$BINARY"

# Check if install dir is in PATH
if ! echo "$PATH" | tr ':' '\n' | grep -q "^$INSTALL_DIR$"; then
    echo ""
    echo "⚠️  $INSTALL_DIR is not in your PATH."
    echo "   Add this to your shell profile (~/.zshrc or ~/.bashrc):"
    echo ""
    echo "   export PATH=\"$INSTALL_DIR:\$PATH\""
    echo ""
fi

echo "✓ Installed $BINARY to $INSTALL_DIR/"
echo ""
echo "Quick start:"
echo "  $BINARY --init        # Create config from template"
echo "  $BINARY               # Start the gateway"
echo "  $BINARY --help        # Show all options"
