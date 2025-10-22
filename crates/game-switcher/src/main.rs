use anyhow::{Context, Result};
use common::constants::ALLIUM_SCREENSHOTS_DIR;
use common::game_info::GameInfo;
use common::retroarch::RetroArchCommand;
use framebuffer::Framebuffer;
use image::{Rgb, RgbImage};
use log::{debug, info, warn};
use std::fs;
use std::path::PathBuf;
use std::time::Duration;

const INPUT_DEVICE: &str = "/dev/input/event0";

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();
    
    info!("=== Game Switcher POC ===");
    info!("Testing key functionality for Miyoo Mini");
    
    // Test 1: Check if game is running
    info!("\n[TEST 1] Checking if game is running...");
    let game_info = GameInfo::load()?;
    match game_info {
        Some(ref info) => {
            info!("✓ Game is running: {}", info.name);
            info!("  Core: {}", info.core);
            info!("  Path: {}", info.path.display());
            info!("  Has menu: {}", info.has_menu);
        }
        None => {
            warn!("✗ No game is currently running");
            info!("  Please launch a RetroArch game first, then run this POC");
            return Ok(());
        }
    }
    
    let game_info = game_info.unwrap();
    
    // Test 2: RetroArch communication
    info!("\n[TEST 2] Testing RetroArch UDP communication...");
    if game_info.has_menu {
        // Test pause
        info!("  Sending PAUSE command...");
        match RetroArchCommand::Pause.send().await {
            Ok(_) => {
                info!("✓ PAUSE sent successfully");
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
            Err(e) => warn!("✗ Failed to send PAUSE: {}", e),
        }
        
        // Test GET_INFO (query status)
        info!("  Sending GET_INFO command...");
        match RetroArchCommand::GetInfo.send_recv().await {
            Ok(Some(response)) => {
                info!("✓ GET_INFO response received:");
                info!("    {}", response);
                parse_retroarch_info(&response);
            }
            Ok(None) => warn!("✗ GET_INFO timed out (RetroArch may not be running)"),
            Err(e) => warn!("✗ Failed to send GET_INFO: {}", e),
        }
        
        // Test GET_STATE_SLOT
        info!("  Querying current state slot...");
        match RetroArchCommand::GetStateSlot.send_recv().await {
            Ok(Some(response)) => {
                info!("✓ Current state slot: {}", response);
            }
            Ok(None) => warn!("✗ GET_STATE_SLOT timed out"),
            Err(e) => warn!("✗ Failed to query state slot: {}", e),
        }
    } else {
        info!("  Skipping RetroArch tests (not a RetroArch game)");
    }
    
    // Test 3: Framebuffer capture
    info!("\n[TEST 3] Testing framebuffer capture...");
    match capture_screenshot(&game_info).await {
        Ok(path) => {
            info!("✓ Screenshot captured successfully");
            info!("  Saved to: {}", path.display());
        }
        Err(e) => warn!("✗ Failed to capture screenshot: {}", e),
    }
    
    // Test 4: Input device detection
    info!("\n[TEST 4] Testing input device access...");
    match std::fs::File::open(INPUT_DEVICE) {
        Ok(_) => info!("✓ Input device accessible: {}", INPUT_DEVICE),
        Err(e) => warn!("✗ Cannot access input device: {}", e),
    }
    
    // Test 5: Create mock game history
    info!("\n[TEST 5] Creating mock game history...");
    match create_mock_history(&game_info).await {
        Ok(path) => {
            info!("✓ Mock history created");
            info!("  Saved to: {}", path.display());
        }
        Err(e) => warn!("✗ Failed to create mock history: {}", e),
    }
    
    // Resume if we paused
    if game_info.has_menu {
        info!("\n[CLEANUP] Resuming game...");
        tokio::time::sleep(Duration::from_millis(500)).await;
        match RetroArchCommand::Unpause.send().await {
            Ok(_) => info!("✓ Game resumed"),
            Err(e) => warn!("✗ Failed to resume: {}", e),
        }
    }
    
    info!("\n=== POC Complete ===");
    info!("Check the logs above for test results.");
    info!("Screenshot saved to: ~/.allium/screenshots/");
    
    Ok(())
}

async fn capture_screenshot(game_info: &GameInfo) -> Result<PathBuf> {
    let fb = Framebuffer::new("/dev/fb0").context("Failed to open framebuffer")?;
    
    debug!("Framebuffer info: {}x{} @ {}bpp",
        fb.var_screen_info.xres,
        fb.var_screen_info.yres,
        fb.var_screen_info.bits_per_pixel
    );
    
    let x0 = fb.var_screen_info.xoffset as usize;
    let y0 = fb.var_screen_info.yoffset as usize;
    let w = fb.var_screen_info.xres as usize;
    let h = fb.var_screen_info.yres as usize;
    let bpp = fb.var_screen_info.bits_per_pixel as usize / 8;
    
    let mut image = RgbImage::new(w as u32, h as u32);
    let frame = fb.read_frame();
    
    // Convert framebuffer to image (with 180° rotation for Miyoo Mini)
    for y in 0..h {
        for x in 0..w {
            let i = ((y0 + y) * w + (x0 + x)) * bpp;
            let pixel = Rgb([frame[i + 2], frame[i + 1], frame[i]]);
            image.put_pixel((w - x - 1) as u32, (h - y - 1) as u32, pixel);
        }
    }
    
    // Generate filename based on game
    fs::create_dir_all(ALLIUM_SCREENSHOTS_DIR.as_path())?;
    let filename = format!("poc_{}_{}.png",
        game_info.core.replace(' ', "_"),
        chrono::Utc::now().format("%Y%m%d_%H%M%S")
    );
    let path = ALLIUM_SCREENSHOTS_DIR.join(filename);
    
    image.save(&path)?;
    
    Ok(path)
}

fn parse_retroarch_info(response: &str) {
    info!("  Parsing GET_INFO response...");
    
    // GET_INFO returns format like: "GET_INFO CONTENT_LOADED,rom_name.ext"
    // or "GET_INFO PAUSED,rom_name.ext" etc.
    let parts: Vec<&str> = response.split_whitespace().collect();
    
    if parts.len() >= 2 && parts[0] == "GET_INFO" {
        let rest = parts[1..].join(" ");
        let info_parts: Vec<&str> = rest.split(',').collect();
        
        if !info_parts.is_empty() {
            let state = info_parts[0];
            info!("    State: {}", state);
            
            if info_parts.len() > 1 {
                let content = info_parts[1..].join(",");
                info!("    Content: {}", content);
            }
        }
    } else {
        debug!("    Unexpected format: {}", response);
    }
}

async fn create_mock_history(current_game: &GameInfo) -> Result<PathBuf> {
    use serde::Serialize;
    
    #[derive(Serialize)]
    struct HistoryEntry {
        name: String,
        path: String,
        core: String,
        last_played: String,
    }
    
    let history_dir = common::constants::ALLIUM_BASE_DIR.join("state");
    fs::create_dir_all(&history_dir)?;
    let path = history_dir.join("game_history_poc.json");
    
    let entries = vec![
        HistoryEntry {
            name: current_game.name.clone(),
            path: current_game.path.display().to_string(),
            core: current_game.core.clone(),
            last_played: chrono::Utc::now().to_rfc3339(),
        },
        HistoryEntry {
            name: "Mock Game 1".to_string(),
            path: "/mnt/SDCARD/Roms/GB/mock1.gb".to_string(),
            core: "gambatte".to_string(),
            last_played: (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339(),
        },
        HistoryEntry {
            name: "Mock Game 2".to_string(),
            path: "/mnt/SDCARD/Roms/GBA/mock2.gba".to_string(),
            core: "mgba".to_string(),
            last_played: (chrono::Utc::now() - chrono::Duration::hours(5)).to_rfc3339(),
        },
    ];
    
    let json = serde_json::to_string_pretty(&entries)?;
    fs::write(&path, json)?;
    
    Ok(path)
}
