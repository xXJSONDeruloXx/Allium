use std::fmt;
use std::path::PathBuf;
use std::{collections::HashMap, path::Path};

use anyhow::{Context, Result, anyhow, bail};
use common::command::Command;
use common::database::Database;
use common::game_history::{GameHistory, GameHistoryEntry};
use common::game_info::GameInfo;
use serde::Deserialize;

use common::constants::{ALLIUM_CONFIG_CONSOLES, ALLIUM_CONFIG_CORES, ALLIUM_RETROARCH};
use log::{debug, error, trace};

use crate::entry::game::Game;

pub type CoreName = String;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Console {
    /// The name of the console.
    pub name: String,
    /// List of cores to use. First is default.
    #[serde(default)]
    pub cores: Vec<CoreName>,
    /// Folder/file names to match against. If the folder/file matches exactly OR contains a parenthesized string that matches exactly, this core will be used.
    /// e.g. "GBA" matches "GBA", "Game Boy Advance (GBA)"
    #[serde(default)]
    pub patterns: Vec<String>,
    /// File extensions to match against. This matches against all extensions, if there are multiple.
    /// e.g. "gba" matches "Game.gba", "Game.GBA", "Game.gba.zip"
    #[serde(default)]
    pub extensions: Vec<String>,
    /// File names to match against. This matches against the entire file name, including extension.
    /// e.g. "Doukutsu.exe" for NXEngine
    #[serde(default)]
    pub file_name: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct ConsoleConfig {
    consoles: Vec<Console>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Core {
    /// Name of core for display.
    pub name: String,
    /// The kind of core: RetroArch, Path
    #[serde(flatten)]
    pub core: CoreType,
    /// Whether swap should be enabled.
    #[serde(default)]
    pub swap: bool,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum CoreType {
    /// Name of the RetroArch core.
    RetroArch(String),
    /// Path of launch script.
    Path(PathBuf),
}

impl fmt::Display for Core {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.name)
    }
}

#[derive(Debug, Deserialize)]
struct CoresConfig {
    cores: HashMap<CoreName, Core>,
}

#[derive(Debug, Clone)]
pub struct ConsoleMapper {
    cores: HashMap<CoreName, Core>,
    consoles: Vec<Console>,
}

impl Default for ConsoleMapper {
    fn default() -> Self {
        Self::new()
    }
}

impl ConsoleMapper {
    pub fn new() -> ConsoleMapper {
        ConsoleMapper {
            cores: HashMap::new(),
            consoles: Vec::new(),
        }
    }

    pub fn load_config(&mut self) -> Result<()> {
        let consoles = std::fs::read_to_string(ALLIUM_CONFIG_CONSOLES.as_path()).map_err(|e| {
            anyhow!(
                "Failed to load consoles config: {:?}, {}",
                &*ALLIUM_CONFIG_CONSOLES,
                e
            )
        })?;
        let consoles: ConsoleConfig =
            toml::from_str(&consoles).context("Failed to parse consoles.toml.")?;
        self.consoles = consoles.consoles;

        let cores = std::fs::read_to_string(ALLIUM_CONFIG_CORES.as_path()).map_err(|e| {
            anyhow!(
                "Failed to load cores config: {:?}, {}",
                &*ALLIUM_CONFIG_CORES,
                e
            )
        })?;
        let cores: CoresConfig = toml::from_str(&cores).context("Failed to parse cores.toml.")?;
        self.cores = cores.cores;

        Ok(())
    }

    /// Returns a console that matches the directory name exactly, or none.
    pub fn get_console_by_dir(&self, path: &Path) -> Option<&Console> {
        if let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) {
            let console = self
                .consoles
                .iter()
                .find(|core| core.patterns.iter().any(|s| name == s));
            if console.is_some() {
                return console;
            }
        }

        None
    }

    /// Returns a console that this path maps to, or none.
    pub fn get_console(&self, path: &Path) -> Option<&Console> {
        let path_lowercase = path.as_os_str().to_ascii_lowercase();

        if let Some(name) = path.file_name().and_then(std::ffi::OsStr::to_str) {
            let console = self
                .consoles
                .iter()
                .find(|core| core.file_name.iter().any(|s| name == s));
            if console.is_some() {
                return console;
            }
        }

        if let Some(extensions) = path_lowercase.to_str() {
            for ext in extensions.split('.').skip(1) {
                let console = self
                    .consoles
                    .iter()
                    .find(|core| core.extensions.iter().any(|s| s == ext));
                if console.is_some() {
                    return console;
                }
            }
        }

        let mut parent = Some(path);
        while let Some(path) = parent {
            trace!("path: {:?}", path);
            if let Some(filename) = path.file_name().and_then(std::ffi::OsStr::to_str) {
                let console = self.consoles.iter().find(|core| {
                    core.patterns.iter().any(|pattern| {
                        filename == pattern || filename.contains(&format!("({})", pattern))
                    })
                });
                if console.is_some() {
                    return console;
                }
            }
            parent = path.parent();
        }

        None
    }

    pub fn launch_game(
        &self,
        database: &Database,
        game: &mut Game,
        disable_savestate_auto_load: bool,
    ) -> Result<Option<Command>> {
        if !game.path.exists()
            && let Some(old) = Game::resync(&mut game.path)?
        {
            database.update_game_path(&old, &game.path)?;
        }

        let image = game.image().map(Path::to_path_buf);
        database.increment_play_count(&game.clone().into())?;

        let console = self.get_console(game.path.as_path());
        let Some(console) = console else {
            bail!(
                "Console for game \"{}\" does not exist.",
                game.path.to_string_lossy()
            );
        };
        let Some(core_name) = game.core.as_ref().or_else(|| console.cores.first()) else {
            return Ok(None);
        };
        let Some(core) = self.cores.get(core_name) else {
            error!("Core \"{}\" does not exist.", core_name);
            return Ok(None);
        };
        let game_info = match &core.core {
            CoreType::RetroArch(libretro_core) => GameInfo::new(
                game.name.clone(),
                game.path.clone(),
                core_name.clone(),
                image,
                if disable_savestate_auto_load {
                    ALLIUM_RETROARCH
                        .parent()
                        .unwrap()
                        .join("launch_without_savestate_auto_load.sh")
                        .display()
                        .to_string()
                } else {
                    ALLIUM_RETROARCH.display().to_string()
                },
                vec![libretro_core.to_string(), game.path.display().to_string()],
                true,
                core.swap,
            ),
            CoreType::Path(path) => GameInfo::new(
                game.name.clone(),
                game.path.clone(),
                core_name.clone(),
                image,
                path.to_string_lossy().to_string(),
                vec![game.path.display().to_string()],
                false,
                core.swap,
            ),
        };
        debug!("Saving game info: {:?}", game_info);
        game_info.save()?;
        
        // Record game launch in history
        debug!("Recording game in history: {}", game_info.name);
        let history = GameHistory::new(database.clone());
        let history_entry = GameHistoryEntry::new(
            game_info.name.clone(),
            game_info.path.clone(),
            game_info.core.clone(),
            game_info.command.clone(),
            game_info.args.clone(),
            None, // Screenshot will be captured later when switching games
            game_info.has_menu,
            game_info.needs_swap,
        );
        match history.record_launch(history_entry) {
            Ok(_) => debug!("Game successfully recorded in history"),
            Err(e) => error!("Failed to record game in history: {}", e),
        }
        
        Ok(Some(Command::Exec(game_info.command())))
    }

    pub fn get_core_name(&self, core: &str) -> String {
        self.cores
            .get(core)
            .map(|s| s.to_string())
            .unwrap_or_else(|| core.to_string())
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use super::*;
    use serial_test::serial;

    #[test]
    fn test_console_mapper() {
        let mut mapper = ConsoleMapper::new();
        mapper.consoles = vec![Console {
            name: "Test".into(),
            patterns: vec!["POKE".into(), "PKM".into()],
            extensions: vec!["gb".into(), "gbc".into()],
            cores: vec![],
            file_name: vec![],
        }];

        assert!(mapper.get_console(Path::new("Roms/POKE/rom.zip")).is_some());
        assert!(mapper.get_console(Path::new("Roms/PKM/rom.zip")).is_some());
        assert!(
            mapper
                .get_console(Path::new("Roms/Pokemon Mini (POKE)/rom.zip"))
                .is_some()
        );
        assert!(
            mapper
                .get_console(Path::new("Roms/POKE MINI/rom.zip"))
                .is_none()
        );
        assert!(mapper.get_console(Path::new("Roms/rom.gb")).is_some());
        assert!(mapper.get_console(Path::new("Roms/rom.gbc")).is_some());
        assert!(mapper.get_console(Path::new("Roms/rom.gbc.zip")).is_some());
        assert!(mapper.get_console(Path::new("Roms/rom.zip.gbc")).is_some());
        assert!(mapper.get_console(Path::new("Roms/gbc")).is_none());
        assert!(mapper.get_console(Path::new("Roms/rom.gba")).is_none());
    }

    #[test]
    #[serial(env_ALLIUM_BASE_DIR)]
    fn test_config() {
        // SAFETY: tests that depend on this env var are run serially
        unsafe {
            env::set_var("ALLIUM_BASE_DIR", "../../static/.allium");
        }

        let mut mapper = ConsoleMapper::new();
        mapper.load_config().unwrap();

        let eq = |rom: &str, console_name: &str, core: &str| -> bool {
            let console = mapper.get_console(Path::new(rom));
            if console.is_none() {
                println!("No console found for {}", rom);
                return false;
            }
            let console = console.unwrap();
            if console.name == console_name && console.cores.first() == Some(&core.to_string()) {
                true
            } else {
                println!(
                    "Expected console: {} core: {:?}, got console: {} core: {}",
                    console_name,
                    console.cores.first(),
                    console.name,
                    core
                );
                false
            }
        };

        // GB
        assert!(eq("GB/rom.zip", "Game Boy", "gambatte"));
        assert!(eq("rom.gb", "Game Boy", "gambatte"));

        // GBC
        assert!(eq("GBC/rom.zip", "Game Boy Color", "gambatte"));
        assert!(eq("rom.gbc", "Game Boy Color", "gambatte"));

        // GBA
        assert!(eq("GBA/rom.zip", "Game Boy Advance", "gpsp"));
        assert!(eq("rom.gba", "Game Boy Advance", "gpsp"));

        // NES
        assert!(eq("FC/rom.zip", "NES", "fceumm"));
        assert!(eq("NES/rom.zip", "NES", "fceumm"));
        assert!(eq("rom.nes", "NES", "fceumm"));

        // SNES
        assert!(eq("SFC/rom.zip", "SNES", "mednafen_supafaust"));
        assert!(eq("SNES/rom.zip", "SNES", "mednafen_supafaust"));
        assert!(eq("rom.sfc", "SNES", "mednafen_supafaust"));
        assert!(eq("rom.smc", "SNES", "mednafen_supafaust"));

        // PS1
        assert!(eq("PSX/rom.zip", "PlayStation", "pcsx_rearmed"));
        assert!(eq("PS1/rom.zip", "PlayStation", "pcsx_rearmed"));
        assert!(eq("PS/rom.zip", "PlayStation", "pcsx_rearmed"));
        assert!(eq("PS/playlist.m3u", "PlayStation", "pcsx_rearmed"));
        assert!(eq("rom.pbp", "PlayStation", "pcsx_rearmed"));

        // Neo Geo Pocket
        assert!(eq("NGP/rom", "Neo Geo Pocket Color", "mednafen_ngp"));
        assert!(eq("NGC/rom", "Neo Geo Pocket Color", "mednafen_ngp"));
        assert!(eq("rom.ngp", "Neo Geo Pocket Color", "mednafen_ngp"));
        assert!(eq("rom.ngc", "Neo Geo Pocket Color", "mednafen_ngp"));

        // Sega - Game Gear
        assert!(eq("GG/rom", "Game Gear", "picodrive"));
        assert!(eq("rom.gg", "Game Gear", "picodrive"));

        // NXEngine
        assert!(eq("Cave Story/Doukutsu.exe", "Cave Story", "nxengine"));
        assert!(eq("Cave Story (NXENGINE).m3u", "Cave Story", "nxengine"));
    }

    #[test]
    #[serial(env_ALLIUM_BASE_DIR)]
    fn test_core_names() {
        // SAFETY: tests that depend on this env var are run serially
        unsafe {
            env::set_var("ALLIUM_BASE_DIR", "../../static/.allium");
        }

        let mut mapper = ConsoleMapper::new();
        mapper.load_config().unwrap();

        let cores = &mapper.cores;
        for console in mapper.consoles {
            for core in console.cores {
                assert!(cores.contains_key(&core), "Core {} not found", core);
            }
        }
    }
}
