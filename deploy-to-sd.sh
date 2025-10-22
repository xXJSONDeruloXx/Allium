#!/bin/bash
# Deploy Allium to SD card with game switcher integration

SD_CARD="/run/media/kurt/ALLIUM"
DIST_DIR="./dist"

echo "=================================="
echo "Deploying Allium to SD Card"
echo "=================================="
echo ""

# Check if SD card is mounted
if [ ! -d "$SD_CARD" ]; then
    echo "ERROR: SD card not found at $SD_CARD"
    echo "Please mount your SD card and try again"
    exit 1
fi

# Check if build completed
if [ ! -f "target/arm-unknown-linux-gnueabihf/release/allium-menu" ]; then
    echo "ERROR: Build not complete. Run 'make build' first"
    exit 1
fi

echo "SD Card found: $SD_CARD"
echo ""

# Build dist if needed
if [ ! -d "$DIST_DIR" ]; then
    echo "Creating dist directory..."
    make dist
fi

# Copy binaries to dist
echo "Copying binaries to dist..."
cp target/arm-unknown-linux-gnueabihf/release/alliumd dist/.allium/bin/
cp target/arm-unknown-linux-gnueabihf/release/allium-launcher dist/.allium/bin/
cp target/arm-unknown-linux-gnueabihf/release/allium-menu dist/.allium/bin/
cp target/arm-unknown-linux-gnueabihf/release/activity-tracker dist/.allium/bin/
cp target/arm-unknown-linux-gnueabihf/release/screenshot dist/.allium/bin/
cp target/arm-unknown-linux-gnueabihf/release/say dist/.allium/bin/
cp target/arm-unknown-linux-gnueabihf/release/show dist/.allium/bin/
cp target/arm-unknown-linux-gnueabihf/release/show-hotkeys dist/.allium/bin/
cp target/arm-unknown-linux-gnueabihf/release/myctl dist/.allium/bin/

echo ""
echo "Backing up existing .allium directory..."
if [ -d "$SD_CARD/.allium" ]; then
    BACKUP_NAME=".allium.backup.$(date +%Y%m%d_%H%M%S)"
    cp -r "$SD_CARD/.allium" "$SD_CARD/$BACKUP_NAME"
    echo "Backup created: $BACKUP_NAME"
fi

echo ""
echo "Copying to SD card..."
rsync -av --progress "$DIST_DIR/.allium/" "$SD_CARD/.allium/"

echo ""
echo "=================================="
echo "Deployment Complete!"
echo "=================================="
echo ""
echo "Changes made:"
echo "  - Updated allium-menu with 'Switch Game' menu option"
echo "  - Added locale string for 'Switch Game'"
echo "  - All other Allium binaries updated"
echo ""
echo "To test:"
echo "  1. Safely eject SD card"
echo "  2. Insert into Miyoo Mini"
echo "  3. Power on and launch a RetroArch game"
echo "  4. Press MENU button"
echo "  5. You should see 'Switch Game' option!"
echo ""
echo "Note: Selecting 'Switch Game' will show a warning"
echo "      (not yet implemented) - that's expected for now."
echo ""
