# Game Switcher POC - Quick Start Guide

## What Is This?

A proof-of-concept binary that tests the core functionality needed for GameSwitcher on Miyoo Mini hardware. This validates:

- ✅ Game detection (via GameInfo)
- ✅ RetroArch UDP communication (PAUSE, GET_INFO, UNPAUSE)
- ✅ Framebuffer screenshot capture
- ✅ Input device access
- ✅ File I/O for game history

## Quick Start (3 Steps)

### 1. Build the POC

```bash
make game-switcher
```

This creates `dist/game-switcher` ready for your Miyoo Mini.

### 2. Copy to Device

```bash
# Copy binary and test script
scp dist/game-switcher root@<device-ip>:/tmp/
scp crates/game-switcher/test.sh root@<device-ip>:/tmp/
```

### 3. Test on Device

SSH into your Miyoo Mini:

```bash
ssh root@<device-ip>
cd /tmp

# Launch a RetroArch game first (any game)
# Then run the test:
sh test.sh
```

## What You'll See

The POC will:
1. Detect the running game
2. Pause RetroArch
3. Query game state via UDP
4. Capture a screenshot
5. Create mock history file
6. Resume the game

All tests log their results, showing ✓ for success or ✗ for failures.

## View the Screenshot

From your PC:

```bash
scp root@<device-ip>:/root/.allium/screenshots/poc_*.png ./screenshot.png
```

Then open `screenshot.png` to verify the framebuffer capture worked!

## Expected Results

If everything works, you should see:
- ✅ All 5 tests pass
- ✅ Game pauses and resumes smoothly
- ✅ Screenshot file created in `~/.allium/screenshots/`
- ✅ Mock history JSON in `~/.allium/state/`

## Troubleshooting

**"No game is currently running"**
→ Launch a RetroArch game before running the POC

**"GET_INFO timed out"**
→ Check RetroArch is running: `ps aux | grep retroarch`

**"Failed to open framebuffer"**
→ Run as root or check `/dev/fb0` permissions

## Next Steps

Once this POC passes, we know:
1. All core functionality works on hardware
2. RetroArch communication is reliable
3. Screenshot capture produces good images
4. No blockers for full implementation

The next phase will build the interactive UI with:
- Game list navigation
- Screenshot display
- D-pad controls
- Game switching

## Files

- `crates/game-switcher/src/main.rs` - POC source code
- `crates/game-switcher/test.sh` - Test script
- `crates/game-switcher/README.md` - Detailed documentation
- `Makefile` - Build target: `make game-switcher`

## Cleanup

After testing, you can delete:
- `/tmp/game-switcher`
- `/tmp/test.sh`
- `~/.allium/screenshots/poc_*.png`
- `~/.allium/state/game_history_poc.json`
