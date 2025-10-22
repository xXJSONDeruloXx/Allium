# Game Switcher - Integrated Approach (BETTER!)

## What Changed

Instead of a separate binary, we're integrating GameSwitcher **directly into the existing ingame menu**. Much cleaner!

## User Experience

When you press MENU during a game, you see:

```
┌───────────────────────────────────┐
│ Super Mario Bros            🔋 92% │
├───────────────────────────────────┤
│ ▶ Continue                        │
│   Switch Game              ← NEW! │
│   Save                            │
│   Load                            │
│   Guide                           │
│   Settings                        │
│   Reset                           │
│   Quit                            │
└───────────────────────────────────┘
    A: Select  B: Back
```

Select "Switch Game" to see:

```
┌───────────────────────────────────┐
│ Switch Game                 🔋 92% │
├───────────────────────────────────┤
│        [Screenshot of Game]       │
│                                   │
│   ◄   Super Mario Bros    ►       │
│       (NES - FCEUmm)              │
│       Last played: 5 min ago      │
│                                   │
│ 2/5                               │
└───────────────────────────────────┘
    A: Switch  B: Back  L/R: Navigate
```

Navigate with D-pad to see other recent games:
- Left/Right: Switch between games
- A: Launch selected game (auto-saves current first)
- B: Cancel and return to menu
- X: Remove from history

## Implementation Benefits

### ✅ Advantages

1. **No separate binary** - All in `allium-menu`
2. **Reuses existing UI** - Same styling, fonts, resources
3. **Natural flow** - Menu → Switch Game → Back to Menu
4. **Clean architecture** - GameSwitcher is just another View (like TextReader for Guide)
5. **Easy to test** - Just launch a game, press MENU, select "Switch Game"
6. **Maintains state** - ingame menu already tracks RetroArch info

### 🏗️ Architecture

```
IngameMenu
├─ Continue       → Exit menu
├─ Switch Game    → child = Some(GameSwitcherView::new(...))
├─ Save           → SaveState
├─ Load           → LoadState
├─ Guide          → child = Some(TextReader::new(...))
├─ Settings       → Open RetroArch menu
├─ Reset          → Reset game
└─ Quit           → Quit to launcher
```

GameSwitcherView works like TextReader:
- Created as `self.child`
- Renders full-screen
- Handles own input
- Returns to menu when done

## Code Changes Made

### 1. MenuEntry Enum
```rust
pub enum MenuEntry {
    Continue,
    SwitchGame,  // ← NEW
    Save,
    Load,
    // ...
}
```

### 2. Locale String
```fluent
# static/.allium/locales/en-US/main.ftl
ingame-menu-switch-game = Switch Game
```

### 3. Handler (Placeholder)
```rust
MenuEntry::SwitchGame => {
    // TODO: Implement GameSwitcherView
    warn!("Game switcher not yet implemented");
}
```

## Next Steps for Full Implementation

### Phase 1: GameSwitcherView (2-3 days)

Create `crates/allium-menu/src/view/game_switcher.rs`:

```rust
pub struct GameSwitcherView<B: Battery> {
    rect: Rect,
    res: Resources,
    history: GameHistory,
    current_index: usize,
    screenshots: HashMap<PathBuf, DynamicImage>,
    // ...
}

impl<B: Battery> View for GameSwitcherView<B> {
    fn draw(&mut self, display: &mut impl Display, styles: &Stylesheet) -> Result<bool> {
        // Render screenshot carousel
        // Show game name, core, playtime
        // Navigation hints
    }
    
    fn handle_key_event(...) -> Result<()> {
        // Left/Right: navigate games
        // A: switch to game
        // B: back to menu
    }
}
```

### Phase 2: Game History (1-2 days)

Create `crates/common/src/game_history.rs`:

```rust
pub struct GameHistory {
    games: VecDeque<RecentGame>,
}

impl GameHistory {
    pub fn load() -> Result<Self>;
    pub fn add_current_game(game_info: &GameInfo) -> Result<()>;
    pub fn get_recent(limit: usize) -> Vec<RecentGame>;
}
```

Storage: `~/.allium/state/game_history.json`

### Phase 3: Screenshot Capture (1 day)

Reuse existing screenshot tool code:

```rust
async fn capture_current_game(game_info: &GameInfo) -> Result<PathBuf> {
    // Use code from crates/screenshot/src/main.rs
    // Save to ~/.allium/screenshots/[hash].png
}
```

### Phase 4: Game Switching (2 days)

```rust
async fn switch_to_game(game: &RecentGame) -> Result<()> {
    // 1. Auto-save current game
    RetroArchCommand::SaveStateSlot(-1).send().await?;
    
    // 2. Quit RetroArch
    RetroArchCommand::Quit.send().await?;
    
    // 3. Update GameInfo
    let new_game_info = GameInfo::from_recent_game(game);
    new_game_info.save()?;
    
    // 4. Launch new game
    Command::Exec(new_game_info.command()).execute()?;
}
```

## Testing Plan

### Manual Test (5 minutes)

1. Build: `make`
2. Copy to device
3. Launch a RetroArch game
4. Press MENU
5. See "Switch Game" option
6. Select it
7. Verify placeholder message shows

### Full Test (once implemented)

1. Play several games (creates history)
2. Launch a game
3. Press MENU → Switch Game
4. Navigate through recent games
5. Select different game
6. Verify:
   - Current game auto-saved
   - New game launched correctly
   - History updated
   - Screenshot captured

## Estimated Timeline

- ✅ **Done:** Menu integration (30 min)
- 🚧 **Phase 1:** GameSwitcherView UI (2-3 days)
- 🚧 **Phase 2:** Game History (1-2 days)
- 🚧 **Phase 3:** Screenshot capture (1 day)
- 🚧 **Phase 4:** Game switching (2 days)
- 🚧 **Polish:** Animations, error handling (2-3 days)

**Total:** 8-11 days for full implementation

**POC:** Can test menu integration immediately!

## Comparison with Standalone Binary

| Aspect | Standalone Binary | Integrated (This Approach) |
|--------|------------------|---------------------------|
| Code complexity | Higher (new binary, IPC) | Lower (reuse ingame menu) |
| User experience | Separate hotkey | Natural menu flow |
| Resource sharing | Duplicate | Shared with menu |
| Testing | Need separate test setup | Test with game + MENU |
| Maintenance | Two binaries to maintain | Single codebase |
| Build time | Additional target | Part of allium-menu |

**Winner:** Integrated approach is clearly better!

## Can Test NOW!

You can test the menu integration right now:

```bash
make
# Copy to device
# Launch game, press MENU
# See "Switch Game" option (shows warning for now)
```

Then we implement the GameSwitcherView incrementally!
