# Game Switcher Implementation for Allium

## Overview
This document describes the implementation of a game switcher feature for Allium, inspired by Onion OS's game switching capability. The game switcher allows users to switch between recently played games without returning to the launcher, similar to alt-tab functionality in desktop environments.

## Key Features Implemented

### 1. Game History Tracking (`crates/common/src/game_history.rs`)
- **New database schema**: Added `game_history` table to SQLite database
- **GameHistoryEntry struct**: Stores game metadata including:
  - Game name, path, core
  - Command and arguments
  - Screenshot path (optional)
  - `has_menu` flag (RetroArch games)
  - `needs_swap` flag
  - Last played timestamp
- **GameHistory API**:
  - `record_launch()`: Records when a game is launched
  - `get_recent_games()`: Retrieves N most recent games, excluding current game
  - `get_all_history()`: Gets complete game history
  - Automatic pruning to maintain max 10 entries

### 2. GameSwitcher View (`crates/allium-menu/src/view/game_switcher.rs`)
- **Full-screen UI**: Large screenshot area with bottom control bar
- **Navigation**: Left/Right D-pad to cycle through games
- **Visual feedback**:
  - Current game screenshot (placeholder for now)
  - Game name bar with arrows and counter (e.g., "3/10")
  - Button hints: A to select, B to cancel
- **Empty state**: Handles case when no games in history

### 3. In-Game Menu Integration (`crates/allium-menu/src/view/ingame_menu.rs`)
- **New menu item**: "Switch Game" added to in-game menu (SELECT+MENU)
- **Key binding**: X button opens game switcher
- **Child view management**: GameSwitcher is spawned as child view of IngameMenu

### 4. Game Switching Logic
The actual game switch process:
1. **Auto-save current game**: If RetroArch game, sends SaveStateSlot command (slot 0)
2. **Quit RetroArch**: Sends Quit command with delay for graceful shutdown
3. **Update GameInfo**: Creates new GameInfo for selected game
4. **Save to disk**: Persists new game info to state file
5. **Spawn new process**: Directly spawns the new game process
6. **Exit menu**: Sends Exit command to close all menu UI

## Database Schema

```sql
CREATE TABLE IF NOT EXISTS game_history (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    name TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    core TEXT NOT NULL,
    command TEXT NOT NULL,
    args TEXT NOT NULL,      -- JSON array
    screenshot TEXT,          -- Optional path to screenshot
    has_menu BOOLEAN NOT NULL,
    needs_swap BOOLEAN NOT NULL,
    timestamp INTEGER NOT NULL  -- Unix timestamp
);

CREATE INDEX IF NOT EXISTS idx_game_history_timestamp 
    ON game_history(timestamp DESC);
```

## Key Files Modified/Created

### New Files
1. **`crates/common/src/game_history.rs`** - Complete game history management
2. **`crates/allium-menu/src/view/game_switcher.rs`** - UI and switching logic

### Modified Files
1. **`crates/common/src/database.rs`** - Added game_history table creation
2. **`crates/allium-menu/src/view/ingame_menu.rs`** - Added "Switch Game" menu item and X button handler
3. **`crates/allium-menu/src/view/mod.rs`** - Exported game_switcher module
4. **`crates/common/src/lib.rs`** - Exported game_history module
5. **`crates/allium-launcher/src/allium_launcher.rs`** - Record game launches to history

## Onion OS Inspiration

### What We Studied from Onion OS
Based on reviewing Onion OS's implementation:
- **GSP (Game Switcher Process)**: Separate binary that manages game switching
- **State management**: Uses `/tmp/` files to track current game state
- **Quick switch**: SELECT+X key combination triggers switcher
- **Process management**: Kills current game, updates state, launches new game
- **Screenshot integration**: Shows actual game screenshots in switcher UI

### Our Allium Approach (Differences)
1. **Integrated design**: GameSwitcher is a View within allium-menu, not separate binary
2. **Database-driven**: Uses SQLite for persistent history vs. temporary files
3. **Resource-based**: Leverages Allium's Resources pattern for state sharing
4. **RetroArch-aware**: Special handling for RetroArch games (auto-save states)
5. **View hierarchy**: Follows Allium's existing View pattern with proper event bubbling

## Technical Challenges & Solutions

### Challenge 1: Game State Persistence
- **Problem**: Need to track game metadata across launches
- **Solution**: Created GameHistory with SQLite backend, records at launch time

### Challenge 2: Process Management
- **Problem**: Need to cleanly exit current game and start new one
- **Solution**: 
  - Use RetroArchCommand for graceful shutdown
  - Add delays for save/quit operations
  - Spawn new process directly via Command::spawn()
  - Exit menu to release resources

### Challenge 3: Screenshot Display
- **Problem**: Need to show game screenshots in switcher
- **Solution**: 
  - Store screenshot paths in database
  - Placeholder implementation for now (shows "No Screenshot")
  - TODO: Integrate with existing screenshot system

### Challenge 4: View Integration
- **Problem**: How to integrate switcher into existing menu hierarchy
- **Solution**:
  - Make GameSwitcher a child view of IngameMenu
  - Use CloseView command for proper cleanup
  - Follow existing View trait pattern

## Current Limitations & TODOs

### Not Yet Implemented
1. **Screenshot display**: Need to load and render actual screenshots
2. **Screenshot capture**: Need to automatically capture when switching away
3. **State slot management**: Currently hardcoded to slot 0
4. **Error handling**: Limited feedback when switch fails
5. **Non-RetroArch games**: May need special handling for standalone emulators
6. **Transition animations**: UI is immediate, could be smoother

### Known Issues
1. **Race conditions**: Minimal delays may not be sufficient on slow hardware
2. **State file conflicts**: If launcher reads while switcher writes
3. **Memory usage**: All history entries loaded into memory
4. **No game validation**: Doesn't check if ROM file still exists

## Testing Approach

### Manual Testing Checklist
1. ✅ Launch multiple different games to build history
2. ✅ Open in-game menu (SELECT+MENU)
3. ✅ Navigate to "Switch Game" or press X
4. ✅ Verify games list appears
5. ✅ Test Left/Right navigation
6. ✅ Test A button to switch
7. ✅ Test B button to cancel
8. ✅ Verify current game state is saved (RetroArch)
9. ✅ Verify new game launches correctly
10. ✅ Test with empty history (no crash)

### Edge Cases to Test
- Switching between same game
- Rapid button presses during switch
- Switching with low battery
- Switching with USB connected
- Games that don't exist anymore
- Corrupted game history database

## Integration with Existing Allium Systems

### Resources Pattern
```rust
// GameInfo is stored in Resources
let current_game = res.get::<GameInfo>();

// Database is accessed via Resources
let db = res.get::<Database>().clone();
```

### Command Pattern
```rust
// RetroArch commands
RetroArchCommand::SaveStateSlot(0).send().await?;
RetroArchCommand::Quit.send().await?;

// Menu commands
commands.send(Command::CloseView).await?;
commands.send(Command::Exit).await?;
```

### View Trait
```rust
impl View for GameSwitcher {
    fn draw(...) -> Result<bool>;
    fn handle_key_event(...) -> Result<bool>;
    fn should_draw(&self) -> bool;
    // ... etc
}
```

## Performance Considerations

1. **Database queries**: Keep history size limited (max 10 entries)
2. **Screenshot loading**: Should be async/lazy loaded
3. **UI rendering**: Only redraw on dirty flag
4. **Process spawning**: Minimal delay between quit and launch

## Future Enhancements

### Short Term
1. Implement actual screenshot display
2. Add screenshot capture on game switch
3. Better error messages/feedback
4. Loading spinner during switch

### Long Term
1. Configurable state slot selection
2. Game switcher history filtering (by console/core)
3. Favorite games quick-switch
4. Switch animation/transition effects
5. Multi-state management (save multiple slots per game)
6. Cloud sync for game history

## Code Examples

### Recording a Game Launch
```rust
use common::game_history::{GameHistory, GameHistoryEntry};

let entry = GameHistoryEntry::new(
    game_info.name.clone(),
    game_info.path.clone(),
    core.clone(),
    command.clone(),
    args.clone(),
    None, // screenshot
    game_info.has_menu,
    game_info.needs_swap,
);

let history = GameHistory::new(db);
history.record_launch(entry)?;
```

### Getting Recent Games
```rust
let current_game_path = Some(current_game.path.clone());
let recent = history.get_recent_games(current_game_path.as_ref(), 9)?;
```

### Switching to a Game
```rust
async fn switch_to_game(&self, commands: Sender<Command>) -> Result<()> {
    let game = &self.games[self.selected];
    
    // Auto-save current game state
    RetroArchCommand::SaveStateSlot(0).send().await?;
    tokio::time::sleep(Duration::from_millis(500)).await;
    
    // Quit RetroArch
    RetroArchCommand::Quit.send().await?;
    tokio::time::sleep(Duration::from_millis(1000)).await;
    
    // Create and save new game info
    let new_game_info = GameInfo::new(/* ... */);
    new_game_info.save()?;
    
    // Launch new game
    let mut cmd = new_game_info.command();
    cmd.spawn()?;
    
    // Exit menu
    commands.send(Command::Exit).await?;
    
    Ok(())
}
```

## Migration Path for Fresh Branch

### Step 1: Database Setup
1. Add game_history table to database schema
2. Add migration in database initialization

### Step 2: History Tracking
1. Create `game_history.rs` module
2. Add to `crates/common/src/lib.rs`
3. Hook into launcher to record game launches

### Step 3: UI Components
1. Create `game_switcher.rs` view
2. Add to view module exports
3. Create ChildView enum variant in ingame_menu

### Step 4: Menu Integration
1. Add "Switch Game" menu item to ingame_menu
2. Add X button handler to spawn GameSwitcher
3. Implement CloseView command handling

### Step 5: Testing & Refinement
1. Test on hardware
2. Adjust timing delays as needed
3. Add error handling
4. Implement screenshot support

## Conclusion

This implementation provides a solid foundation for game switching in Allium. The core functionality works, but there's room for polish and additional features. The design follows Allium's existing patterns and should integrate cleanly into the codebase.

The key insight from studying Onion OS was the importance of state management and process lifecycle handling. Our implementation adapts these concepts to Allium's architecture while maintaining consistency with existing code patterns.
