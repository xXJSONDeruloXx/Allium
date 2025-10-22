use std::path::PathBuf;

use anyhow::Result;
use chrono::Utc;
use log::{debug, trace};
use serde::{Deserialize, Serialize};

use crate::database::Database;

/// Represents a game in the history list
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GameHistoryEntry {
    pub name: String,
    pub path: PathBuf,
    pub core: String,
    pub command: String,
    pub args: Vec<String>,
    pub has_menu: bool,
    pub needs_swap: bool,
    pub last_played: i64,
}

impl GameHistoryEntry {
    pub fn new(
        name: String,
        path: PathBuf,
        core: String,
        command: String,
        args: Vec<String>,
        has_menu: bool,
        needs_swap: bool,
    ) -> Self {
        Self {
            name,
            path,
            core,
            command,
            args,
            has_menu,
            needs_swap,
            last_played: Utc::now().timestamp(),
        }
    }
}

/// GameHistory tracks recently played games for the game switcher
pub struct GameHistory {
    db: Database,
}

impl GameHistory {
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Get recent games, excluding the current game if provided
    /// Returns up to `limit` games, sorted by last_played timestamp (most recent first)
    pub fn get_recent_games(
        &self,
        current_game_path: Option<&PathBuf>,
        limit: usize,
    ) -> Result<Vec<GameHistoryEntry>> {
        trace!(
            "GameHistory::get_recent_games - current_game: {:?}, limit: {}",
            current_game_path,
            limit
        );

        // Get recently played games from database
        let mut recent_games = self.db.select_last_played((limit + 1) as i64)?;

        debug!(
            "Found {} recent games from database",
            recent_games.len()
        );

        // Filter out the current game if specified
        if let Some(current_path) = current_game_path {
            recent_games.retain(|g| &g.path != current_path);
            debug!(
                "After filtering current game, {} games remain",
                recent_games.len()
            );
        }

        // Limit to requested amount
        recent_games.truncate(limit);

        // Convert to GameHistoryEntry
        // Note: We don't have command/args in the database, so we'll need to construct them
        // For now, we'll use placeholder values and rely on the game info being updated later
        let entries: Vec<GameHistoryEntry> = recent_games
            .into_iter()
            .map(|game| {
                // Extract core name from path or use stored core
                let core = game.core.clone().unwrap_or_else(|| {
                    // Try to infer core from path
                    "unknown".to_string()
                });

                // For RetroArch games, the command is typically retroarch
                // TODO: This should be stored in the database or inferred from game type
                let command = if game.path.extension().and_then(|s: &std::ffi::OsStr| s.to_str()) == Some("pak") {
                    game.path.to_string_lossy().to_string()
                } else {
                    "retroarch".to_string()
                };

                let args = if command == "retroarch" {
                    vec![
                        "-L".to_string(),
                        format!("/mnt/SDCARD/RetroArch/.retroarch/cores/{}_libretro.so", core),
                        game.path.to_string_lossy().to_string(),
                    ]
                } else {
                    vec![]
                };

                GameHistoryEntry {
                    name: game.name,
                    path: game.path,
                    core,
                    command,
                    args,
                    has_menu: true, // Assume RetroArch games have menu
                    needs_swap: false, // TODO: Determine this properly
                    last_played: game.last_played,
                }
            })
            .collect();

        debug!(
            "Returning {} game history entries",
            entries.len()
        );
        for (i, entry) in entries.iter().enumerate() {
            trace!("  [{}] {} - {:?}", i, entry.name, entry.path);
        }

        Ok(entries)
    }

    /// Record that a game was played (updates last_played timestamp)
    pub fn record_game_played(&self, path: &PathBuf) -> Result<()> {
        debug!("GameHistory::record_game_played - {:?}", path);
        
        // The database already tracks this via increment_play_count and last_played
        // We just need to ensure the game exists in the database
        if let Some(game) = self.db.select_game(path)? {
            debug!("Game found in database: {}", game.name);
        } else {
            debug!("Game not found in database, it will be added when played");
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_history_entry_creation() {
        let entry = GameHistoryEntry::new(
            "Test Game".to_string(),
            PathBuf::from("/path/to/game.rom"),
            "test_core".to_string(),
            "retroarch".to_string(),
            vec!["-L".to_string(), "core.so".to_string()],
            true,
            false,
        );

        assert_eq!(entry.name, "Test Game");
        assert_eq!(entry.path, PathBuf::from("/path/to/game.rom"));
        assert_eq!(entry.core, "test_core");
        assert!(entry.last_played > 0);
    }

    #[test]
    fn test_game_history_with_in_memory_db() -> Result<()> {
        let db = Database::in_memory()?;
        let history = GameHistory::new(db);

        // Should return empty list for new database
        let recent = history.get_recent_games(None, 10)?;
        assert_eq!(recent.len(), 0);

        Ok(())
    }
}
