#!/bin/sh
# Quick test script for Game Switcher POC
# Copy this along with the game-switcher binary to your Miyoo Mini

echo "==================================="
echo "Game Switcher POC Test Script"
echo "==================================="
echo ""

# Check if binary exists
if [ ! -f "./game-switcher" ]; then
    echo "ERROR: game-switcher binary not found in current directory"
    echo "Please copy it here first"
    exit 1
fi

# Make executable
chmod +x ./game-switcher

# Check if a game is running
if [ ! -f "/root/.allium/game.json" ]; then
    echo "WARNING: No game appears to be running"
    echo "Please launch a RetroArch game first, then run this script"
    echo ""
    echo "Press Enter to continue anyway, or Ctrl+C to exit"
    read dummy
fi

# Run with logging
echo "Running POC tests..."
echo ""
RUST_LOG=info ./game-switcher

echo ""
echo "==================================="
echo "Test Results:"
echo "==================================="

# Check for created files
if [ -f "/root/.allium/screenshots/poc_"*.png ]; then
    echo "✓ Screenshot created"
    ls -lh /root/.allium/screenshots/poc_*.png 2>/dev/null | tail -1
else
    echo "✗ No screenshot found"
fi

if [ -f "/root/.allium/state/game_history_poc.json" ]; then
    echo "✓ Mock history created"
    echo "  Sample:"
    head -n 5 /root/.allium/state/game_history_poc.json
else
    echo "✗ No history file found"
fi

echo ""
echo "==================================="
echo "To view the screenshot on your PC:"
echo "  scp root@<device-ip>:/root/.allium/screenshots/poc_*.png ."
echo ""
echo "POC test complete!"
echo "==================================="
