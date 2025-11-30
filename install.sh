#!/bin/bash
set -e

# Install yore to ~/.local/bin
# This follows standard Unix conventions and ~/.local/bin is often in PATH

INSTALL_DIR="$HOME/.local/bin"
BINARY_NAME="yore"

echo "Building yore in release mode..."
cargo build --release

echo "Creating install directory: $INSTALL_DIR"
mkdir -p "$INSTALL_DIR"

echo "Installing yore to $INSTALL_DIR/$BINARY_NAME"
cp target/release/yore "$INSTALL_DIR/$BINARY_NAME"
chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo ""
echo "✓ yore installed successfully!"
echo ""
echo "Installation path: $INSTALL_DIR/$BINARY_NAME"
echo ""

# Check if ~/.local/bin is in PATH
if [[ ":$PATH:" == *":$HOME/.local/bin:"* ]]; then
    echo "✓ $HOME/.local/bin is already in your PATH"
    echo ""
    echo "You can now run: yore --help"
else
    echo "⚠ $HOME/.local/bin is NOT in your PATH"
    echo ""
    echo "Add this line to your ~/.bashrc or ~/.zshrc:"
    echo ""
    echo "    export PATH=\"\$HOME/.local/bin:\$PATH\""
    echo ""
    echo "Then run: source ~/.bashrc (or source ~/.zshrc)"
    echo ""
    echo "Or run yore with full path: $INSTALL_DIR/$BINARY_NAME"
fi

echo ""
echo "To verify installation:"
echo "  $INSTALL_DIR/$BINARY_NAME --version"
