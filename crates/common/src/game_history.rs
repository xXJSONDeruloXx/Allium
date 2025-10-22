use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::database::Database;

/// Maximum number of games to keep in history
const MAX_HISTORY_SIZE: usize = 10;

/// Represents a single entry in the game history
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GameHistoryEntry {
    /// Display name of the game
    pub name: String,
    /// Full path to the game ROM
    pub path: PathBuf,
    /// Core used to run the game
    pub core: String,
    /// Command to run the core
    pub command: String,
    /// Arguments to pass to the core
    pub args: Vec<String>,
    /// Path to the screenshot image (if available)
    pub screenshot: Option<PathBuf>,
    /// Whether the game has an in-game menu (RetroArch)
    pub has_menu: bool,
    /// Whether swap is needed for this game
    pub needs_swap: bool,
    /// When this game was last played
    pub timestamp: DateTime<Utc>,
}

impl GameHistoryEntry {
    /// Create a new game history entry
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: String,
        path: PathBuf,
        core: String,
        command: String,
        args: Vec<String>,
        screenshot: Option<PathBuf>,
        has_menu: bool,
        needs_swap: bool,
    ) -> Self {
        Self {
            name,
            path,
            core,
            command,
            args,
            screenshot,
            has_menu,
            needs_swap,
            timestamp: Utc::now(),
        }
    }
}

/// Manages game launch history
pub struct GameHistory {
    db: Database,
}

impl GameHistory {
    /// Create a new GameHistory instance
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// Record a game launch in the history
    pub fn record_launch(&self, entry: GameHistoryEntry) -> Result<()> {
        let screenshot = entry.screenshot.as_ref().map(|p| p.display().to_string());
        let args_json = serde_json::to_string(&entry.args)?;

        self.db.conn().execute(
            "INSERT INTO game_history (name, path, core, command, args, screenshot, has_menu, needs_swap, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path) DO UPDATE SET
                name = ?1,
                core = ?3,
                command = ?4,
                args = ?5,
                screenshot = ?6,
                has_menu = ?7,
                needs_swap = ?8,
                timestamp = ?9",
            rusqlite::params![
                entry.name,
                entry.path.display().to_string(),
                entry.core,
                entry.command,
                args_json,
                screenshot,
                entry.has_menu,
                entry.needs_swap,
                entry.timestamp.timestamp(),
            ],
        )?;

        // Keep only the most recent MAX_HISTORY_SIZE entries
        self.cleanup_old_entries()?;

        Ok(())
    }

    /// Get the most recent N games from history (excluding the currently playing game)
    pub fn get_recent_games(&self, current_game_path: Option<&PathBuf>, limit: usize) -> Result<Vec<GameHistoryEntry>> {
        let mut query = String::from(
            "SELECT name, path, core, command, args, screenshot, has_menu, needs_swap, timestamp
             FROM game_history"
        );

        // Exclude the current game if provided
        if current_game_path.is_some() {
            query.push_str(" WHERE path != ?1");
        }

        query.push_str(" ORDER BY timestamp DESC LIMIT ?");

        let mut stmt = self.db.conn().prepare(&query)?;

        let entries = if let Some(current_path) = current_game_path {
            stmt.query_map(
                rusqlite::params![current_path.display().to_string(), limit],
                Self::map_row,
            )?
        } else {
            stmt.query_map(rusqlite::params![limit], Self::map_row)?
        };

        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }

        Ok(result)
    }

    /// Get all games in history, ordered by most recent
    pub fn get_all(&self) -> Result<Vec<GameHistoryEntry>> {
        let mut stmt = self.db.conn().prepare(
            "SELECT name, path, core, command, args, screenshot, has_menu, needs_swap, timestamp
             FROM game_history
             ORDER BY timestamp DESC"
        )?;

        let entries = stmt.query_map([], Self::map_row)?;
        let mut result = Vec::new();
        for entry in entries {
            result.push(entry?);
        }

        Ok(result)
    }

    /// Update the screenshot for a specific game
    pub fn update_screenshot(&self, path: &PathBuf, screenshot: PathBuf) -> Result<()> {
        self.db.conn().execute(
            "UPDATE game_history SET screenshot = ?1 WHERE path = ?2",
            rusqlite::params![
                screenshot.display().to_string(),
                path.display().to_string()
            ],
        )?;
        Ok(())
    }

    /// Remove old entries to keep history size manageable
    fn cleanup_old_entries(&self) -> Result<()> {
        self.db.conn().execute(
            "DELETE FROM game_history
             WHERE id NOT IN (
                SELECT id FROM game_history
                ORDER BY timestamp DESC
                LIMIT ?1
             )",
            rusqlite::params![MAX_HISTORY_SIZE],
        )?;
        Ok(())
    }

    /// Map a database row to a GameHistoryEntry
    fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<GameHistoryEntry> {
        let args_json: String = row.get(4)?;
        let args: Vec<String> = serde_json::from_str(&args_json).unwrap_or_default();
        let screenshot: Option<String> = row.get(5)?;
        let timestamp: i64 = row.get(8)?;

        Ok(GameHistoryEntry {
            name: row.get(0)?,
            path: PathBuf::from(row.get::<_, String>(1)?),
            core: row.get(2)?,
            command: row.get(3)?,
            args,
            screenshot: screenshot.map(PathBuf::from),
            has_menu: row.get(6)?,
            needs_swap: row.get(7)?,
            timestamp: DateTime::from_timestamp(timestamp, 0)
                .unwrap_or_else(|| Utc::now()),
        })
    }

    /// Clear all history
    #[allow(dead_code)]
    pub fn clear(&self) -> Result<()> {
        self.db.conn().execute("DELETE FROM game_history", [])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_game_history() -> Result<()> {
        let db = Database::in_memory()?;
        let history = GameHistory::new(db);

        // Record a game
        let entry = GameHistoryEntry::new(
            "Test Game".to_string(),
            PathBuf::from("/test/game.gb"),
            "gambatte".to_string(),
            "retroarch".to_string(),
            vec!["-L".to_string(), "gambatte.so".to_string()],
            None,
            true,
            false,
        );

        history.record_launch(entry.clone())?;

        // Verify it's in history
        let recent = history.get_recent_games(None, 10)?;
        assert_eq!(recent.len(), 1);
        assert_eq!(recent[0].name, "Test Game");

        Ok(())
    }

    #[test]
    fn test_history_limit() -> Result<()> {
        let db = Database::in_memory()?;
        let history = GameHistory::new(db);

        // Add more than MAX_HISTORY_SIZE games
        for i in 0..15 {
            let entry = GameHistoryEntry::new(
                format!("Game {}", i),
                PathBuf::from(format!("/test/game{}.gb", i)),
                "core".to_string(),
                "retroarch".to_string(),
                vec![],
                None,
                true,
                false,
            );
            history.record_launch(entry)?;
        }

        // Should only keep MAX_HISTORY_SIZE
        let all = history.get_all()?;
        assert_eq!(all.len(), MAX_HISTORY_SIZE);

        Ok(())
    }
}
