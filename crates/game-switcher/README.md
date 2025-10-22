# Game Switcher POC

This is a proof-of-concept implementation to validate core GameSwitcher functionality on Miyoo Mini hardware.

## What This Tests

1. **Game Detection** - Verifies if a game is currently running via GameInfo
2. **RetroArch Communication** - Tests UDP commands (PAUSE, GET_INFO, GET_STATE_SLOT)
3. **Framebuffer Capture** - Captures a screenshot of the current game
4. **Input Access** - Verifies input device accessibility
5. **History Management** - Creates mock game history file

## Building

### For Miyoo Mini (cross-compile):

```bash
# From Allium root directory
make game-switcher
```

This will create `dist/game-switcher` ready to copy to your device.

### For local testing (x86_64):

```bash
cargo build --bin game-switcher
```

## Running on Miyoo Mini

1. **Launch a RetroArch game** (any game will work)

2. **SSH into your device** or use a terminal:
   ```bash
   ssh root@<device-ip>
   ```

3. **Copy the binary** to your device:
   ```bash
   scp dist/game-switcher root@<device-ip>:/tmp/
   ```

4. **While the game is running, execute**:
   ```bash
   cd /tmp
   RUST_LOG=info ./game-switcher
   ```

## Expected Output

You should see output like:

```
=== Game Switcher POC ===
Testing key functionality for Miyoo Mini

[TEST 1] Checking if game is running...
✓ Game is running: Super Mario Bros
  Core: fceumm
  Path: /mnt/SDCARD/Roms/NES/mario.nes
  Has menu: true

[TEST 2] Testing RetroArch UDP communication...
  Sending PAUSE command...
✓ PAUSE sent successfully
  Sending GET_INFO command...
✓ GET_INFO response received:
    GET_INFO PAUSED,/path/to/rom.ext
  Parsing GET_INFO response...
    State: PAUSED
    Content: /path/to/rom.ext
  Querying current state slot...
✓ Current state slot: GET_STATE_SLOT 0

[TEST 3] Testing framebuffer capture...
✓ Screenshot captured successfully
  Saved to: /root/.allium/screenshots/poc_fceumm_20251022_143052.png

[TEST 4] Testing input device access...
✓ Input device accessible: /dev/input/event0

[TEST 5] Creating mock game history...
✓ Mock history created
  Saved to: /root/.allium/state/game_history_poc.json

[CLEANUP] Resuming game...
✓ Game resumed

=== POC Complete ===
Check the logs above for test results.
Screenshot saved to: ~/.allium/screenshots/
```

## Verification

After running, check:

1. **Screenshot file** - Should exist at `~/.allium/screenshots/poc_*.png`
   - Copy it back to view: `scp root@<device-ip>:/root/.allium/screenshots/poc_*.png .`
   
2. **Game resumed** - The game should unpause and continue running

3. **Mock history** - Check `~/.allium/state/game_history_poc.json` for the JSON structure

## Troubleshooting

### "No game is currently running"
- Make sure you launch a RetroArch game before running the POC
- Check that `/root/.allium/game.json` exists

### "GET_INFO timed out"
- RetroArch might not be responding to UDP commands
- Check that RetroArch is running: `ps aux | grep retroarch`
- Ensure RetroArch is configured to listen on UDP port 55355

### "Failed to open framebuffer"
- Make sure you have permission to access `/dev/fb0`
- Try running as root: `sudo ./game-switcher`

### Game doesn't resume
- Manually send unpause: `echo "UNPAUSE" | nc -u 127.0.0.1 55355`
- Or restart the game

## What's Next

If all tests pass, we have validated:
- ✅ Game state detection works
- ✅ RetroArch UDP communication works  
- ✅ Framebuffer capture works
- ✅ Input device is accessible
- ✅ File I/O for history works

The next step is building the interactive UI that:
- Shows recent games list
- Allows navigation with D-pad
- Displays game screenshots
- Handles switching between games

## Files Created

- `~/.allium/screenshots/poc_*.png` - Screenshot of running game
- `~/.allium/state/game_history_poc.json` - Mock game history

You can safely delete these after testing.
