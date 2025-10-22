# GameSwitcher Implementation Evaluation

## 📋 Executive Summary

**Status:** ✅ **WELL-PAVED** - Implementation is feasible with minimal unknowns

**Key Findings:**
- 🎉 **Framebuffer capture already implemented** (`crates/screenshot/src/main.rs`)
- 🎉 **Game state tracking already exists** (`GameInfo` + `is_ingame()` in alliumd)
- 🎉 **RetroArch UDP commands already working** (just need GET_INFO parsing)
- 🎉 **Display save/restore built into trait** (perfect for overlay mode)
- 🟡 **Main work is UI adaptation** to embedded-graphics (40% of effort)
- 🟡 **Minor unknowns** around performance tuning and GET_INFO format

**Risk Level:** 🟡 Medium-Low (85% confidence)

**Estimated Effort:** 5-7 weeks full implementation, 2 weeks for proof-of-concept

**Recommendation:** ✅ **PROCEED** - Start with 2-week POC to validate approach

---

## Overview

This document evaluates what would be needed to port Onion's GameSwitcher functionality to Allium. The Onion repository has been added as a reference submodule at `third-party/Onion`.

After thorough analysis, the implementation path is **well-understood** with most required infrastructure **already present** in Allium.

## Onion GameSwitcher Architecture

### Core Functionality

The GameSwitcher in Onion is a C-based application using SDL 1.2 that provides:

1. **In-Game Overlay** - Activates while a RetroArch game is running
2. **Game History Management** - Shows recently played games with screenshots
3. **Quick Switching** - Switch between games without returning to main menu
4. **State Management** - Auto-saves before switching, loads states when resuming
5. **Visual Feedback** - Game screenshots, playtime, battery, brightness controls
6. **Multiple View Modes** - Normal (with UI), Minimal, and Fullscreen views

### Key Components

Located in `third-party/Onion/src/gameSwitcher/`:

| File | Purpose |
|------|---------|
| `gameSwitcher.c` | Main entry point and event loop |
| `gs_model.h` | Data structures (Game_s, RecentItem) |
| `gs_history.h` | History extraction from recent games list |
| `gs_overlay.h` | Overlay mode for switching during gameplay |
| `gs_retroarch.h` | RetroArch integration and config parsing |
| `gs_keystate.h` | Input handling and hotkeys |
| `gs_render.h` | UI rendering logic |
| `gs_romscreen.h` | Screenshot loading and scaling |
| `gs_popMenu.h` | Save state menu functionality |

### Technical Details

**Data Sources:**
- Reads from: `/mnt/SDCARD/Saves/CurrentProfile/lists/content_history.lpl` (RetroArch history)
- Screenshots stored in: `/mnt/SDCARD/Saves/CurrentProfile/romScreens/`
- Config overrides from: `/mnt/SDCARD/Saves/CurrentProfile/config/`

**RetroArch Communication:**
- Uses UDP socket (port 55355) to send commands
- Commands: PAUSE, UNPAUSE, SAVE_STATE_SLOT, LOAD_STATE_SLOT, GET_INFO, etc.
- Implements autosave (SAVE_STATE_SLOT -1)

**Display Management:**
- Captures framebuffer when pausing game
- Saves as PNG screenshot hashed by ROM path
- Scales screenshots based on aspect ratio settings
- Supports custom header/footer themes

**Workflow:**
1. User presses MENU button during gameplay
2. RetroArch pauses
3. Framebuffer captured as screenshot
4. GameSwitcher overlay appears with recent games list
5. User navigates left/right through games
6. On selection: auto-save current game, quit RetroArch, launch new game
7. On back: resume current game

## Allium Current Architecture

### Existing Infrastructure

**Strengths:**
- ✅ Written in Rust with strong type safety
- ✅ Already has RetroArch UDP command infrastructure (`crates/common/src/retroarch.rs`)
- ✅ Has GameInfo system for tracking current game state
- ✅ Has database for game metadata and history
- ✅ Has in-game menu system (`allium-menu`)
- ✅ Async/await architecture with Tokio
- ✅ Display abstraction layer with embedded-graphics
- ✅ Screenshot directory already defined (`ALLIUM_SCREENSHOTS_DIR`)

**Gaps:**
- ❌ No game history/recent games tracking
- ❌ No screenshot capture from framebuffer
- ❌ No overlay mode for switching during gameplay
- ❌ No RetroArch config override parsing
- ❌ Limited RetroArch state queries (no GET_INFO parsing)

### Relevant Crates

```
crates/
├── common/              # Shared functionality
│   ├── retroarch.rs    # RetroArch UDP commands (already exists!)
│   ├── game_info.rs    # Game tracking (needs enhancement)
│   ├── database.rs     # SQLite database (can extend)
│   └── display/        # Display abstractions
├── allium-menu/        # In-game menu
│   ├── ingame_menu.rs  # Could integrate here
│   └── retroarch_info.rs
└── allium-launcher/    # Game launcher
    └── consoles.rs     # Launch logic
```

## Implementation Plan

### Phase 1: Data Layer Enhancements

#### 1.1 Game History Tracking

**Create:** `crates/common/src/game_history.rs`

```rust
pub struct RecentGame {
    pub name: String,
    pub rom_path: PathBuf,
    pub core_path: String,
    pub core_name: String,
    pub screenshot_path: Option<PathBuf>,
    pub last_played: DateTime<Utc>,
    pub play_time: Duration,
}

pub struct GameHistory {
    games: VecDeque<RecentGame>,
    max_size: usize,
}

impl GameHistory {
    pub fn load() -> Result<Self>;
    pub fn save(&self) -> Result<()>;
    pub fn add_game(&mut self, game: RecentGame);
    pub fn get_recent(&self, count: usize) -> &[RecentGame];
    pub fn remove_game(&mut self, path: &Path);
}
```

**Storage Format:**
- JSON file at `~/.allium/state/game_history.json`
- Similar structure to RetroArch's content_history.lpl but simpler
- Cap at 100 most recent games (matching Onion's MAX_HISTORY)

#### 1.2 Screenshot Management

**Extend:** `crates/common/src/display/mod.rs`

```rust
pub trait Display {
    // ... existing methods ...
    
    /// Capture current framebuffer to buffer
    fn capture_framebuffer(&self) -> Result<Vec<u8>>;
    
    /// Save framebuffer as PNG
    fn save_screenshot(&self, path: &Path) -> Result<()>;
}
```

**Create:** `crates/common/src/screenshot.rs`

```rust
pub struct ScreenshotManager {
    screenshots_dir: PathBuf,
}

impl ScreenshotManager {
    pub fn new() -> Self;
    
    /// Generate screenshot filename from ROM path
    pub fn screenshot_path(&self, rom_path: &Path, core: &str) -> PathBuf;
    
    /// Save screenshot with game metadata
    pub fn save(&self, display: &impl Display, game: &GameInfo) -> Result<PathBuf>;
    
    /// Load screenshot for game
    pub fn load(&self, game: &RecentGame) -> Result<Option<DynamicImage>>;
    
    /// Delete screenshot
    pub fn delete(&self, path: &Path) -> Result<()>;
}
```

### Phase 2: RetroArch Integration Enhancements

#### 2.1 Extend RetroArch Commands

**Modify:** `crates/common/src/retroarch.rs`

Add new command variants:
```rust
pub enum RetroArchCommand {
    // ... existing variants ...
    
    /// Auto-save to slot -1
    AutoSave,
    
    /// Query RetroArch for current state and content info
    GetStatus,
}

pub struct RetroArchStatus {
    pub state: RetroArchState,
    pub content_name: Option<String>,
    pub core_path: Option<String>,
}

pub enum RetroArchState {
    Playing,
    Paused,
    Contentless,
    Unknown,
}

impl RetroArchCommand {
    pub async fn get_status() -> Result<RetroArchStatus> {
        // Parse GET_INFO response
    }
}
```

#### 2.2 RetroArch Config Parser

**Create:** `crates/common/src/retroarch_config.rs`

```rust
pub struct RetroArchConfig {
    global_config: HashMap<String, String>,
}

impl RetroArchConfig {
    pub fn load() -> Result<Self>;
    
    /// Get config value with override hierarchy:
    /// 1. Game-specific override
    /// 2. Content directory override  
    /// 3. Core override
    /// 4. Global config
    pub fn get_bool_with_overrides(
        &self,
        key: &str,
        core_name: &str,
        rom_path: &Path,
    ) -> Option<bool>;
}
```

This handles the complex RetroArch config hierarchy for features like:
- `video_dingux_ipu_keep_aspect` (aspect ratio)
- `video_scale_integer` (integer scaling)
- `savestate_auto_save` (auto-save enabled)

### Phase 3: Game Switcher UI Component

#### 3.1 Create New Module

**Create:** `crates/allium-menu/src/view/game_switcher.rs`

```rust
pub struct GameSwitcher<B: Battery> {
    rect: Rect,
    res: Resources,
    battery: B,
    
    // State
    history: GameHistory,
    current_index: usize,
    current_screenshot: Option<DynamicImage>,
    
    // View state
    view_mode: ViewMode,
    show_legend: bool,
    show_time: bool,
    brightness_overlay: Option<u8>,
    
    // Child view for save state menu
    child: Option<Box<dyn View>>,
}

pub enum ViewMode {
    Normal,    // Header + footer + game info
    Minimal,   // Just game name
    Fullscreen // Screenshot only
}

impl<B: Battery> GameSwitcher<B> {
    pub async fn activate(
        rect: Rect,
        res: Resources,
        battery: B,
    ) -> Result<Self>;
    
    /// Capture screenshot of currently running game
    async fn capture_current_game(&mut self) -> Result<()>;
    
    /// Switch to selected game
    async fn switch_game(&self, game: &RecentGame) -> Result<()>;
    
    /// Resume current game
    async fn resume_current(&self) -> Result<()>;
    
    /// Auto-save current game state
    async fn autosave_current(&self) -> Result<()>;
}

impl<B: Battery> View for GameSwitcher<B> {
    fn draw(&mut self, display: &mut impl Display, styles: &Stylesheet) -> Result<bool> {
        // Render based on view_mode
        // - Background: current screenshot
        // - Overlay: transparent black
        // - UI elements: game name, arrows, battery, etc.
    }
    
    fn handle_key_event(
        &mut self,
        event: KeyEvent,
        commands: Sender<Command>,
        bubble: &mut VecDeque<Self>,
    ) -> Result<()> {
        // Handle:
        // - Left/Right: navigate games
        // - A: switch to game / resume
        // - B: exit to main menu
        // - X: remove from history
        // - Y: toggle view mode
        // - Menu: toggle save state menu
        // - Up/Down: brightness
    }
}
```

#### 3.2 Screenshot Rendering

The UI needs to render:
1. **Background:** Full-screen screenshot of selected game
2. **Overlay:** Semi-transparent dark overlay for readability
3. **Header** (Normal mode):
   - Battery indicator
   - Clock
   - Current game index (e.g., "3/10")
   - Total playtime
4. **Game Name Bar:**
   - Left/right arrows
   - Scrolling game name
5. **Footer** (Normal mode):
   - Button hints (A: Select, B: Exit, etc.)
6. **Legend** (Auto-hide after 5s):
   - Key bindings

**Rendering Strategy:**
```rust
fn render_screenshot(&self, display: &mut impl Display, screenshot: &DynamicImage) -> Result<()> {
    // Scale screenshot to display size
    // Apply aspect ratio and integer scaling if needed
    // Center on display
}

fn render_overlay(&self, display: &mut impl Display) -> Result<()> {
    // Semi-transparent black rectangle
    // Alpha blend with screenshot
}

fn render_game_name(&self, display: &mut impl Display, game: &RecentGame) -> Result<()> {
    // Dark bar at bottom
    // Left/right arrows if applicable
    // Scrolling text for long names
}
```

### Phase 4: Hotkey Integration

#### 4.1 Detect Trigger

**Modify:** `crates/allium-menu/src/allium_menu.rs`

Add detection for GameSwitcher activation:
```rust
pub struct AlliumMenu<P: Platform> {
    // ... existing fields ...
    game_switcher: Option<GameSwitcher<P::Battery>>,
}

impl AlliumMenu<DefaultPlatform> {
    async fn handle_hotkey(&mut self, event: KeyEvent) -> Result<()> {
        // Check if MENU button pressed during game
        if event == KeyEvent::Menu && self.is_game_running() {
            self.activate_game_switcher().await?;
        }
    }
    
    async fn activate_game_switcher(&mut self) -> Result<()> {
        // 1. Pause RetroArch
        RetroArchCommand::Pause.send().await?;
        
        // 2. Capture screenshot
        let screenshot_path = self.capture_screenshot().await?;
        
        // 3. Update game history
        let mut history = GameHistory::load()?;
        history.add_game(/* current game */);
        history.save()?;
        
        // 4. Show GameSwitcher UI
        self.game_switcher = Some(
            GameSwitcher::activate(self.rect, self.res.clone(), self.battery.clone()).await?
        );
        
        Ok(())
    }
}
```

#### 4.2 Daemon/Background Service

Consider creating a background service (similar to Onion's `keymon`):

**Create:** `crates/game-switcher-daemon/`

```rust
/// Monitors for MENU button press and launches game switcher
pub struct GameSwitcherDaemon {
    // Monitor input device
    // Check if RetroArch is running
    // Launch game switcher on trigger
}
```

This could run as a separate process launched by `alliumd`.

### Phase 5: State Transition Logic

#### 5.1 Game Switching Flow

```rust
impl<B: Battery> GameSwitcher<B> {
    async fn switch_game(&self, new_game: &RecentGame) -> Result<()> {
        // 1. Show "SAVING" message
        self.show_message("SAVING...")?;
        
        // 2. Auto-save current game
        RetroArchCommand::AutoSave.send().await?;
        
        // 3. Wait for save to complete
        tokio::time::sleep(Duration::from_millis(500)).await;
        
        // 4. Quit RetroArch
        RetroArchCommand::Quit.send().await?;
        
        // 5. Update GameInfo for new game
        let new_game_info = GameInfo::new(
            new_game.name.clone(),
            new_game.rom_path.clone(),
            new_game.core_name.clone(),
            // ... other fields
        );
        new_game_info.save()?;
        
        // 6. Show "LOADING" message
        self.show_message("LOADING...")?;
        
        // 7. Launch new game
        Command::Exec(new_game_info.command()).execute()?;
        
        Ok(())
    }
    
    async fn resume_current(&self) -> Result<()> {
        // Just unpause RetroArch
        RetroArchCommand::Unpause.send().await?;
        Ok(())
    }
}
```

#### 5.2 Save State Menu

When user presses MENU while in GameSwitcher:

```rust
pub struct SaveStateMenu {
    slots: Vec<SaveStateSlot>,
    selected_slot: usize,
}

pub struct SaveStateSlot {
    slot_number: i8,
    exists: bool,
    screenshot: Option<PathBuf>,
    timestamp: Option<DateTime<Utc>>,
}

impl SaveStateMenu {
    pub async fn new(game: &RecentGame) -> Result<Self> {
        // Scan for .state0 through .state9 files
        // Check for screenshots
        // Load metadata
    }
    
    async fn handle_save(&self, slot: i8) -> Result<()> {
        RetroArchCommand::SetStateSlot(slot).send().await?;
        RetroArchCommand::SaveState.send().await?;
    }
    
    async fn handle_load(&self, slot: i8) -> Result<()> {
        RetroArchCommand::SetStateSlot(slot).send().await?;
        RetroArchCommand::LoadState.send().await?;
    }
}
```

### Phase 6: Database Extensions

#### 6.1 Add Recent Games Table

**Modify:** `crates/common/src/database.rs`

```rust
impl Database {
    pub fn migrations<'a>() -> Migrations<'a> {
        Migrations::new(vec![
            // ... existing migrations ...
            M::up("
                CREATE TABLE IF NOT EXISTS recent_games (
                    id INTEGER PRIMARY KEY,
                    game_id INTEGER NOT NULL,
                    played_at INTEGER NOT NULL,
                    core_name TEXT NOT NULL,
                    core_path TEXT NOT NULL,
                    screenshot_path TEXT,
                    FOREIGN KEY (game_id) REFERENCES games(id)
                );
                
                CREATE INDEX idx_recent_games_played_at ON recent_games(played_at DESC);
            "),
        ])
    }
    
    pub fn get_recent_games(&self, limit: usize) -> Result<Vec<RecentGame>> {
        // Query with JOIN to games table
    }
    
    pub fn add_recent_game(&self, game_id: i64, core_name: &str, core_path: &str) -> Result<()> {
        // Insert or update
    }
}
```

Alternatively, keep recent games in a separate JSON file for simplicity and to match Onion's approach.

## Architecture Comparison

| Aspect | Onion | Allium | Adaptation Strategy |
|--------|-------|--------|---------------------|
| Language | C | Rust | Rewrite logic in Rust |
| Graphics | SDL 1.2 | embedded-graphics | Port rendering to embedded-graphics |
| Async | None (blocking) | Tokio async | Wrap blocking ops in spawn_blocking |
| Display | Direct SDL Surface | Display trait | Implement screenshot capture |
| Input | SDL Events | KeyEvent enum | Map to existing input system |
| Storage | Text files + SQLite | JSON + SQLite | Use same approach |
| Threading | pthreads | Tokio tasks | Use async tasks |

## Key Challenges

### 1. Framebuffer Capture

**Challenge:** Allium's Display trait doesn't currently expose framebuffer reading.

**Solution:** 
- Add `capture_framebuffer()` method to Display trait
- Implement for each platform (miyoomini, simulator)
- On miyoomini: read from `/dev/fb0` directly
- Save as PNG using `image` crate

```rust
// In platform/miyoomini/display.rs
impl Display for MiyoominiDisplay {
    fn capture_framebuffer(&self) -> Result<Vec<u8>> {
        let fb = OpenOptions::new()
            .read(true)
            .open("/dev/fb0")?;
        
        let mut buffer = vec![0u8; self.width * self.height * 4];
        fb.read_exact(&mut buffer)?;
        Ok(buffer)
    }
}
```

### 2. Overlay Rendering

**Challenge:** Need to render UI over the paused game's framebuffer.

**Solution:**
- When activating GameSwitcher, capture current framebuffer
- Use as background image
- Render semi-transparent overlay
- Draw UI elements on top

The Display trait already supports layered rendering via `save()` and `load()` methods.

### 3. RetroArch State Synchronization

**Challenge:** Ensuring RetroArch is in correct state (paused, autosaved, etc.)

**Solution:**
- Add proper async waiting after commands
- Parse GET_INFO response to verify state
- Add timeouts and error handling
- Monitor for RetroArch process termination

```rust
async fn ensure_paused() -> Result<()> {
    RetroArchCommand::Pause.send().await?;
    tokio::time::sleep(Duration::from_millis(100)).await;
    
    let status = RetroArchCommand::GetStatus.send_recv().await?;
    if status.state != RetroArchState::Paused {
        return Err(anyhow!("Failed to pause RetroArch"));
    }
    Ok(())
}
```

### 4. Config Override Hierarchy

**Challenge:** RetroArch has complex config override system.

**Solution:**
- Implement parser for RetroArch config files
- Follow same hierarchy as Onion:
  1. Game-specific: `config/[core]/[game_name].cfg`
  2. Directory: `config/[core]/[dir_name].cfg`
  3. Core: `config/[core]/[core].cfg`
  4. Global: `retroarch.cfg`
- Cache parsed configs for performance

### 5. Screenshot Scaling

**Challenge:** Screenshots need to respect aspect ratio and scaling settings.

**Solution:**
- Use `image` crate for scaling
- Apply same logic as Onion's `scaleRomScreen()`
- Read aspect ratio settings from RetroArch config
- Scale to display dimensions

```rust
fn scale_screenshot(
    screenshot: &DynamicImage,
    config: &RetroArchConfig,
    game: &RecentGame,
) -> DynamicImage {
    let keep_aspect = config.get_bool_with_overrides(
        "video_dingux_ipu_keep_aspect",
        &game.core_name,
        &game.rom_path,
    ).unwrap_or(true);
    
    let integer_scaling = config.get_bool_with_overrides(
        "video_scale_integer", 
        &game.core_name,
        &game.rom_path,
    ).unwrap_or(false);
    
    // Calculate scale factors
    let (width, height) = calculate_scaled_size(
        screenshot.dimensions(),
        (640, 480),
        keep_aspect,
        integer_scaling,
    );
    
    screenshot.resize_exact(width, height, FilterType::Nearest)
}
```

## Testing Strategy

### 1. Unit Tests
- Game history add/remove/get
- Screenshot path generation
- RetroArch command serialization
- Config parser with overrides

### 2. Integration Tests
- Full game switching flow
- Screenshot capture and restore
- RetroArch communication
- Database operations

### 3. Manual Testing
- Test with different cores
- Various aspect ratios
- Multiple games in history
- Battery and brightness display
- Save state menu
- Error conditions (RetroArch crash, etc.)

## Rollout Plan

### Phase 1: Foundations (Week 1-2)
- [ ] Add game history tracking
- [ ] Implement screenshot capture
- [ ] Extend RetroArch commands
- [ ] Create config parser

### Phase 2: UI Development (Week 3-4)
- [ ] Build GameSwitcher view
- [ ] Implement rendering
- [ ] Add input handling
- [ ] Create save state menu

### Phase 3: Integration (Week 5)
- [ ] Integrate with allium-menu
- [ ] Add hotkey detection
- [ ] Wire up state transitions
- [ ] Add error handling

### Phase 4: Polish (Week 6)
- [ ] Add animations/transitions
- [ ] Optimize performance
- [ ] Theme support
- [ ] Documentation

### Phase 5: Testing & Release (Week 7-8)
- [ ] Comprehensive testing
- [ ] Bug fixes
- [ ] User documentation
- [ ] Release

## Key Implementation Details

### Framebuffer Capture - Already Implemented! 🎉

Allium already has screenshot functionality in `crates/screenshot/src/main.rs`:

```rust
fn screenshot(path: impl AsRef<Path>) -> Result<()> {
    let fb = Framebuffer::new("/dev/fb0")?;
    
    // Read framebuffer dimensions
    let x0 = fb.var_screen_info.xoffset as usize;
    let y0 = fb.var_screen_info.yoffset as usize;
    let w = fb.var_screen_info.xres as usize;
    let h = fb.var_screen_info.yres as usize;
    let bpp = fb.var_screen_info.bits_per_pixel as usize / 8;
    
    // Read frame and create image
    let mut image = RgbImage::new(w as u32, h as u32);
    let frame = fb.read_frame();
    
    for y in 0..h {
        for x in 0..w {
            let i = ((y0 + y) * w + (x0 + x)) * bpp;
            let pixel = Rgb([frame[i + 2], frame[i + 1], frame[i]]);
            // Note: 180 degree rotation for Miyoo Mini
            image.put_pixel((w - x - 1) as u32, (h - y - 1) as u32, pixel);
        }
    }
    
    image.save(path)?;
    Ok(())
}
```

**To integrate:** Extract this into a method on `Display` trait or create a `ScreenshotManager` that can be called from GameSwitcher.

### Game State Detection - Already Tracked! 🎉

From `crates/alliumd/src/alliumd.rs` line ~374:

```rust
KeyEvent::Released(Key::Menu) => {
    if self.is_menu_pressed_alone {
        if self.is_ingame()  // ← Already tracks if game is running!
            && let Some(game_info) = GameInfo::load()?  // ← Has game metadata
        {
            // Launch menu
        } else if game_info.has_menu {  // ← Knows if it's RetroArch
            self.launch_menu().await?;
        }
    }
}
```

The `is_ingame()` method checks if a game is currently running by looking at the `ALLIUM_GAME_INFO` file.

### RetroArch Integration - Mostly Done! 🎉

From `crates/common/src/retroarch.rs`:

```rust
impl RetroArchCommand {
    pub async fn send(&self) -> Result<()> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(RETROARCH_UDP_SOCKET).await?;  // 127.0.0.1:55355
        socket.send(self.as_str().as_bytes()).await?;
        Ok(())
    }
    
    pub async fn send_recv(&self) -> Result<Option<String>> {
        let socket = UdpSocket::bind("0.0.0.0:0").await?;
        socket.connect(RETROARCH_UDP_SOCKET).await?;
        socket.send(self.as_str().as_bytes()).await?;
        
        let mut reply = vec![0; 128];
        match tokio::time::timeout(
            Duration::from_millis(250),  // ← Already has timeout!
            socket.recv_from(&mut reply)
        ).await {
            Ok(Ok((len, _))) => {
                reply.truncate(len);
                Ok(Some(String::from_utf8(reply)?))
            }
            _ => Ok(None)
        }
    }
}
```

**Already supported commands:**
- ✅ Pause / Unpause
- ✅ SaveState / LoadState
- ✅ SetStateSlot / GetStateSlot
- ✅ SaveStateSlot / LoadStateSlot (including -1 for autosave!)

**Need to add:**
- GET_INFO (to query state and content)
- Parse the response into a structured format

### Display Save/Restore - Built In! 🎉

From `crates/common/src/platform/miyoo/framebuffer.rs`:

```rust
impl Display for FramebufferDisplay {
    fn save(&mut self) -> Result<()> {
        // Push current buffer to stack
        self.saved.push(self.framebuffer.buffer.clone());
        Ok(())
    }
    
    fn load(&mut self, rect: Rect) -> Result<()> {
        // Restore from stack
        let Some(ref saved) = self.saved.last() else {
            bail!("No saved image");
        };
        // ... copy saved buffer back
    }
    
    fn pop(&mut self) -> bool {
        self.saved.pop();
        !self.saved.is_empty()
    }
}
```

This is perfect for overlay mode:
1. `save()` before showing overlay
2. `load()` when dismissing overlay
3. Can layer UI over saved framebuffer

## Alternative Approaches

### Approach A: Separate Binary (Like Onion)
Create a standalone `game-switcher` binary that:
- Launches as overlay when MENU pressed
- Communicates with allium-menu via IPC
- Exits back to game or launches new game

**Pros:**
- Cleaner separation of concerns
- Can be developed/tested independently
- Similar to proven Onion architecture

**Cons:**
- More complex IPC
- State synchronization issues
- Duplicate code (display, input handling)

### Approach B: Integrated Module (Recommended)
Integrate into allium-menu as a view:
- Part of the same process
- Share Resources and Display
- Direct access to GameInfo and Database

**Pros:**
- Simpler architecture
- Shared infrastructure
- Better integration
- Easier state management

**Cons:**
- Larger binary
- Tighter coupling

**Recommendation:** Use Approach B (Integrated Module) as it fits Allium's architecture better.

### Approach C: Library + Binary
Create a library crate with all logic:
- Can be used by allium-menu
- Can also be standalone binary
- Best of both worlds

**Pros:**
- Maximum flexibility
- Reusable components
- Can do both integrated and standalone

**Cons:**
- Most complex
- May be over-engineering

## Dependencies to Add

```toml
# crates/common/Cargo.toml
[dependencies]
image = "0.24"  # Screenshot loading/saving/scaling
png = "0.17"    # PNG encoding/decoding

# crates/allium-menu/Cargo.toml  
[dependencies]
tokio = { version = "1.0", features = ["time", "sync"] }  # Already present
```

## Potential Issues & Mitigation

| Issue | Mitigation |
|-------|------------|
| RetroArch doesn't respond to UDP | Add timeout and retry logic |
| Screenshot capture fails | Fallback to artwork/box art |
| Game history corrupted | Validate on load, rebuild if needed |
| Memory usage with screenshots | Lazy load, cache only visible games |
| Performance on low-end hardware | Optimize rendering, reduce allocations |
| RetroArch crashes during switch | Detect and handle gracefully |
| Config parsing errors | Use defaults, log warnings |

## Configuration Options

Add to Allium settings:

```json
{
  "game_switcher": {
    "enabled": true,
    "hotkey": "MENU",
    "max_history": 100,
    "show_time": false,
    "show_total": true,
    "view_mode": "normal",
    "legend_timeout_ms": 5000,
    "autosave_enabled": true,
    "screenshot_quality": 85
  }
}
```

## Documentation Needed

1. **User Guide:**
   - How to activate GameSwitcher
   - Navigating the UI
   - Save state management
   - Configuration options

2. **Developer Guide:**
   - Architecture overview
   - Adding new view modes
   - Extending history tracking
   - Testing guidelines

3. **Migration Guide:**
   - For users coming from Onion
   - Differences in behavior
   - Config mapping

## Unknowns & Risk Assessment

After analyzing both codebases, here are the **RESOLVED** and **REMAINING** unknowns:

### ✅ RESOLVED - Well-Understood Areas

1. **Framebuffer Access** ✅
   - **Status:** Already implemented in Allium!
   - Allium has `crates/screenshot/src/main.rs` that reads from `/dev/fb0`
   - `FramebufferDisplay` already has access to framebuffer via `iface.read_frame()`
   - The `framebuffer` crate provides the interface
   - **Action:** Just need to expose this as a method on Display trait

2. **RetroArch UDP Communication** ✅
   - **Status:** Working implementation exists
   - `crates/common/src/retroarch.rs` has async UDP send/recv
   - Timeout handling already implemented (250ms)
   - **Missing:** GET_INFO parsing, but response format is known from Onion

3. **Platform Support** ✅
   - **Status:** Full Miyoo Mini support confirmed
   - Allium supports Miyoo283, Miyoo285 (Flip), Miyoo354 (Plus)
   - Same hardware as Onion uses
   - Display is 640x480 at 32bpp
   - **Action:** No platform-specific concerns

4. **Game Detection** ✅
   - **Status:** Already tracked via GameInfo
   - `alliumd` maintains `ALLIUM_GAME_INFO` file
   - `is_ingame()` method exists in alliumd
   - Knows when game is running, has_menu flag, etc.
   - **Action:** Just need to hook into existing system

5. **Hotkey Detection** ✅
   - **Status:** Menu button handling exists
   - `alliumd` has KeyEvent::Released(Key::Menu) handler at line ~374
   - Already checks `is_ingame()` to launch menu
   - **Action:** Extend to also check for GameSwitcher trigger

### ⚠️ REMAINING UNKNOWNS - Need Investigation

1. **RetroArch GET_INFO Response Format** 🟡 LOW RISK
   - **Unknown:** Exact structure of GET_INFO UDP response
   - **Known:** Onion parses it for state and content_info
   - **Risk:** Low - can test empirically with running RetroArch
   - **Mitigation:** Add debug logging, test with real device
   - **Effort:** 2-4 hours testing and parsing

2. **Screenshot Capture Timing** 🟡 LOW RISK
   - **Unknown:** How long after PAUSE does framebuffer stabilize?
   - **Known:** Onion uses ~100-200ms delays
   - **Risk:** Low - can tune with testing
   - **Mitigation:** Add configurable delays
   - **Effort:** Few hours of testing

3. **Performance on Miyoo Mini** 🟡 MEDIUM RISK
   - **Unknown:** Can device handle loading/scaling multiple screenshots?
   - **Known:** Onion does it successfully
   - **Concerns:**
     - Memory usage with 640x480x4 bytes per screenshot
     - Image scaling performance (image crate vs SDL)
     - Lazy loading strategy needed
   - **Risk:** Medium - may need optimization
   - **Mitigation:**
     - Lazy load screenshots (only load visible +/- 2)
     - Use same scaling as Onion (aspect + integer scaling)
     - Profile memory usage
   - **Effort:** 1-2 days optimization

4. **Async State Transitions** 🟡 MEDIUM RISK
   - **Unknown:** Race conditions between Allium and RetroArch lifecycle
   - **Concerns:**
     - What if RetroArch crashes during switch?
     - What if autosave fails?
     - Timing between quit and launch new game
   - **Risk:** Medium - need robust error handling
   - **Mitigation:**
     - Add comprehensive error handling
     - Watchdog for RetroArch process
     - Timeout on all UDP operations
     - State machine for transitions
   - **Effort:** 2-3 days robust implementation

5. **RetroArch Config Parsing** 🟢 LOW RISK
   - **Unknown:** Edge cases in config file parsing
   - **Known:** Format is simple key=value
   - **Risk:** Low - straightforward parsing
   - **Mitigation:**
     - Use existing parsing patterns from common/locale.rs
     - Default to safe values on parse errors
   - **Effort:** 1 day implementation + testing

6. **Overlay Rendering Over Paused Game** 🟢 LOW RISK
   - **Unknown:** Exact sequence to avoid flicker
   - **Known:** Allium's Display trait has save/load methods
   - **Strategy:**
     1. Capture framebuffer → screenshot
     2. Clear display
     3. Draw screenshot
     4. Draw semi-transparent overlay
     5. Draw UI
   - **Risk:** Low - standard rendering pattern
   - **Effort:** Already architected in Display trait

### 🔍 DISCOVERIES - Advantages Found

1. **Screenshot Tool Already Exists!** 🎉
   - `crates/screenshot/src/main.rs` does framebuffer capture
   - Handles rotation (180 degrees for Miyoo)
   - Has crop and resize functionality
   - Can reuse this logic directly!

2. **Display Trait Has Save/Load** 🎉
   - `save()` pushes current buffer to stack
   - `load()` restores from saved buffer
   - Perfect for overlay implementation!
   - Already tested and working

3. **Game Info Tracking Exists** 🎉
   - `GameInfo` has path, core, command, start_time
   - `alliumd` updates it when launching games
   - Already calculates playtime
   - Just need to add to history when switching

4. **Async Architecture is Better** 🎉
   - Onion uses blocking operations + threads
   - Allium's async is more elegant
   - Timeout handling is cleaner
   - Can use spawn_blocking for image operations

### 📊 Risk Summary

| Area | Risk Level | Confidence | Notes |
|------|-----------|------------|-------|
| Framebuffer capture | 🟢 Low | 95% | Already implemented |
| RetroArch UDP | 🟢 Low | 90% | Working, just needs GET_INFO |
| UI Rendering | 🟡 Medium | 80% | Need to adapt to embedded-graphics |
| State transitions | 🟡 Medium | 75% | Needs robust error handling |
| Performance | 🟡 Medium | 70% | May need optimization |
| Config parsing | 🟢 Low | 85% | Straightforward |
| Platform support | 🟢 Low | 95% | Fully compatible |

**Overall Risk:** 🟡 **Medium-Low**

The implementation is **well-paved** with few true unknowns. Most risks are "known unknowns" that can be addressed through:
- Testing on real hardware
- Iterative optimization
- Learning from Onion's approach

## Conclusion

Implementing GameSwitcher in Allium is **feasible and well-understood**. The main work involves:

1. **~40% effort:** Building the UI component and rendering
2. **~30% effort:** RetroArch integration enhancements (mostly GET_INFO parsing)
3. **~20% effort:** Screenshot and history management (leveraging existing code)
4. **~10% effort:** Testing, optimization, and polish

**Estimated effort:** 5-7 weeks for full implementation with testing and optimization.

**Complexity:** Medium
- Most infrastructure already exists
- Clear reference implementation (Onion)
- Main challenge is UI adaptation to embedded-graphics
- Async architecture actually simplifies some aspects

**Confidence Level:** 85%
- Few true unknowns
- Existing code addresses most concerns
- Clear path forward
- Can start with MVP and iterate

**Value:** High
- One of Onion's most popular features
- Significantly improves user experience
- Sets Allium apart from basic launchers
- Leverages Allium's strengths (async, type safety)

**Recommendation:** **PROCEED** with implementation

Start with Phase 1 (foundations) to:
1. Add GET_INFO command and parsing (1-2 days)
2. Expose framebuffer capture on Display trait (1 day)
3. Build basic game history tracking (2-3 days)
4. Create minimal proof-of-concept UI (3-4 days)

This validates the approach with ~2 weeks effort before committing to full implementation. The POC will reveal any remaining unknowns while building on Allium's solid foundation.
