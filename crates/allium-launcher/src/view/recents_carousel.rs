use std::collections::VecDeque;
use std::path::PathBuf;

use anyhow::Result;
use async_trait::async_trait;
use common::command::Command;
use common::constants::{ALLIUM_SCREENSHOTS_DIR, RECENT_GAMES_LIMIT};
use common::database::Database;
use common::display::Display;
use common::geom::{Alignment, Point, Rect};
use common::locale::Locale;
use common::platform::{DefaultPlatform, Key, KeyEvent, Platform};
use common::resources::Resources;
use common::stylesheet::Stylesheet;
use common::view::{ButtonHint, ButtonIcon, Image, ImageMode, Label, Row, View};
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::consoles::ConsoleMapper;
use crate::entry::game::Game;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RecentsCarouselState {
    pub selected: usize,
}

impl Default for RecentsCarouselState {
    fn default() -> Self {
        Self { selected: 0 }
    }
}

#[derive(Debug)]
pub struct RecentsCarousel {
    rect: Rect,
    res: Resources,
    games: Vec<Game>,
    selected: usize,
    screenshot: Image,
    game_name: Label<String>,
    counter_label: Label<String>,
    button_hints: Row<ButtonHint<String>>,
    up_arrow: Label<String>,
    down_arrow: Label<String>,
    dirty: bool,
}

impl RecentsCarousel {
    pub fn new(rect: Rect, res: Resources, state: RecentsCarouselState) -> Result<Self> {
        let Rect { x, y, w, h } = rect;

        // Load recent games from database
        let games = Self::load_games(&res)?;
        let selected = state.selected.min(games.len().saturating_sub(1));

        let styles = res.get::<Stylesheet>();

        // Full-screen screenshot image that takes up most of the screen
        let screenshot_height = h - 100; // Leave room for game name bar and button hints
        let mut screenshot = Image::empty(
            Rect::new(x, y, w, screenshot_height),
            ImageMode::Contain,
        );
        screenshot.set_alignment(Alignment::Center);

        // Game name label at the bottom with semi-transparent background
        let game_name = Label::new(
            Point::new(x + w as i32 / 2, y + screenshot_height as i32 + 20),
            String::new(),
            Alignment::Center,
            None,
        );

        // Counter label (e.g., "1/10")
        let counter_label = Label::new(
            Point::new(x + w as i32 - 12, y + screenshot_height as i32 + 20),
            String::new(),
            Alignment::Right,
            None,
        );

        // Up/Down arrow indicators
        let up_arrow = Label::new(
            Point::new(x + w as i32 / 2, y + screenshot_height as i32 + 5),
            "▲".to_string(),
            Alignment::Center,
            None,
        );

        let down_arrow = Label::new(
            Point::new(x + w as i32 / 2, y + screenshot_height as i32 + 35),
            "▼".to_string(),
            Alignment::Center,
            None,
        );

        // Button hints at the very bottom
        let button_hints = Row::new(
            Point::new(
                x + 12,
                y + h as i32 - ButtonIcon::diameter(&styles) as i32 - 8,
            ),
            {
                let locale = res.get::<Locale>();
                vec![
                    ButtonHint::new(
                        res.clone(),
                        Point::zero(),
                        Key::A,
                        locale.t("button-select"),
                        Alignment::Left,
                    ),
                    ButtonHint::new(
                        res.clone(),
                        Point::zero(),
                        Key::X,
                        locale.t("sort-search"),
                        Alignment::Left,
                    ),
                ]
            },
            Alignment::Left,
            12,
        );

        drop(styles);

        let mut carousel = Self {
            rect,
            res,
            games,
            selected,
            screenshot,
            game_name,
            counter_label,
            button_hints,
            up_arrow,
            down_arrow,
            dirty: true,
        };

        carousel.update_current_game()?;

        Ok(carousel)
    }

    fn load_games(res: &Resources) -> Result<Vec<Game>> {
        let database = res.get::<Database>();
        let games = database.select_last_played(RECENT_GAMES_LIMIT)?;

        log::debug!("RecentsCarousel: Loaded {} recent games", games.len());

        // Log database game info to file
        use std::io::Write;
        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("/mnt/SDCARD/allium-recents-db.log")
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            writeln!(f, "\n=== Loading games from database at {} ===", ts).ok();
            for game in &games {
                writeln!(f, "Game: {}", game.name).ok();
                writeln!(f, "  Path: {:?}", game.path).ok();
                writeln!(f, "  Core from DB: {:?}", game.core).ok();
            }
        }

        Ok(games
            .into_iter()
            .map(|game| {
                let extension = game
                    .path
                    .extension()
                    .and_then(|e| e.to_str())
                    .unwrap_or_default()
                    .to_owned();
                
                let image = crate::entry::lazy_image::LazyImage::from_path(
                    &game.path,
                    game.image.clone(),
                );
                
                Game {
                    name: game.name.clone(),
                    full_name: game.name,
                    path: game.path,
                    image,
                    extension,
                    core: game.core,
                    rating: game.rating,
                    release_date: game.release_date,
                    developer: game.developer,
                    publisher: game.publisher,
                    genres: game.genres,
                    favorite: game.favorite,
                }
            })
            .collect())
    }

    // Append a line to a log file on the SD card root for easier debugging after eject
    fn sd_log_line(&self, line: &str) {
        use std::io::Write;
        use std::time::{SystemTime, UNIX_EPOCH};

        // Prefer ALLIUM_SD_ROOT if available, otherwise /mnt/SDCARD
        let base = common::constants::ALLIUM_SD_ROOT.clone();
        let log_path = base.join("allium-recents.log");

        if let Ok(mut f) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_path)
        {
            let ts = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or_default();
            let _ = writeln!(f, "{}: {}", ts, line);
        }
    }

    fn update_current_game(&mut self) -> Result<()> {
        if self.games.is_empty() {
            self.screenshot.set_path(None);
            self.game_name.set_text(String::new());
            self.counter_label.set_text(String::new());
            log::debug!("RecentsCarousel: No games to display");
            return Ok(());
        }

        let game = &self.games[self.selected];
        log::info!(
            "RecentsCarousel: Updating to game {}/{}: '{}' at path: {:?}",
            self.selected + 1,
            self.games.len(),
            game.name,
            game.path,
        );

        // Find the most recent screenshot that matches this game's path
        let screenshot_path = Self::find_screenshot_for_game(&game.path);
        
        if screenshot_path.is_some() {
            log::info!("RecentsCarousel: Screenshot found for: {}", game.name);
        } else {
            log::warn!("RecentsCarousel: No screenshot available for: {}", game.name);
        }
        self.screenshot.set_path(screenshot_path);

        // Update game name
        self.game_name.set_text(game.name.clone());

        // Update counter
        self.counter_label
            .set_text(format!("{}/{}", self.selected + 1, self.games.len()));

        self.dirty = true;
        Ok(())
    }

    /// Find the most recent screenshot that matches the given game path
    fn find_screenshot_for_game(game_path: &PathBuf) -> Option<PathBuf> {
        use std::fs;
        use std::io::{BufRead, BufReader};
        
        let manifest_path = ALLIUM_SCREENSHOTS_DIR.join("manifest.txt");
        
        log::debug!("Looking for screenshot for game: {:?}", game_path);
        
        if let Ok(file) = fs::File::open(&manifest_path) {
            let reader = BufReader::new(file);
            let mut matching_entries: Vec<(u64, PathBuf)> = Vec::new();
            
            // Convert game_path to string for comparison
            let game_path_str = game_path.to_string_lossy();
            
            for line in reader.lines().filter_map(|l| l.ok()) {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3 {
                    if let Ok(timestamp) = parts[0].parse::<u64>() {
                        let filename = parts[1];
                        let manifest_game_path = parts[2].trim(); // Trim any whitespace
                        
                        log::debug!("Manifest entry: ts={}, path={}", timestamp, manifest_game_path);
                        
                        // Match by game path
                        if manifest_game_path == game_path_str {
                            let screenshot_path = ALLIUM_SCREENSHOTS_DIR.join(filename);
                            if screenshot_path.exists() {
                                log::debug!("Found matching screenshot: {:?}", screenshot_path);
                                matching_entries.push((timestamp, screenshot_path));
                            }
                        }
                    }
                }
            }
            
            // Sort by timestamp, newest first (reverse order) and return the most recent
            matching_entries.sort_by(|a, b| b.0.cmp(&a.0));
            let result = matching_entries.first().map(|(_, path)| path.clone());
            
            if let Some(ref path) = result {
                log::info!("Selected screenshot: {:?} (from {} matches)", path, matching_entries.len());
            } else {
                log::warn!("No screenshots found for game: {:?}", game_path);
            }
            
            return result;
        } else {
            log::warn!("Could not open manifest file: {:?}", manifest_path);
        }
        
        None
    }

    pub fn save(&self) -> RecentsCarouselState {
        RecentsCarouselState {
            selected: self.selected,
        }
    }

    fn navigate_up(&mut self) -> Result<()> {
        if self.selected > 0 {
            self.selected -= 1;
            self.update_current_game()?;
        }
        Ok(())
    }

    fn navigate_down(&mut self) -> Result<()> {
        if self.selected < self.games.len().saturating_sub(1) {
            self.selected += 1;
            self.update_current_game()?;
        }
        Ok(())
    }

    async fn launch_game(&mut self, commands: Sender<Command>) -> Result<()> {
        if let Some(game) = self.games.get_mut(self.selected) {
            let command = self
                .res
                .get::<ConsoleMapper>()
                .launch_game(&self.res.get(), game, false)?;
            if let Some(cmd) = command {
                commands.send(cmd).await?;
            }
        }
        Ok(())
    }
}

#[async_trait(?Send)]
impl View for RecentsCarousel {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        let mut drawn = false;

        if self.dirty {
            // Clear the entire area with background color
            display.load(self.rect)?;
            self.dirty = false;
            drawn = true;
        }

        // Draw the screenshot
        if self.screenshot.should_draw() {
            drawn |= self.screenshot.draw(display, styles)?;
        }

        // Draw semi-transparent overlay for game name area
        if self.games.is_empty() {
            // Show empty state
            let locale = self.res.get::<Locale>();
            let mut empty_label = Label::new(
                Point::new(
                    self.rect.x + self.rect.w as i32 / 2,
                    self.rect.y + self.rect.h as i32 / 2,
                ),
                locale.t("no-recent-games"),
                Alignment::Center,
                None,
            );
            drawn |= empty_label.draw(display, styles)?;
        } else {
            // Draw game name
            if self.game_name.should_draw() {
                drawn |= self.game_name.draw(display, styles)?;
            }

            // Draw counter
            if self.counter_label.should_draw() {
                drawn |= self.counter_label.draw(display, styles)?;
            }

            // Draw arrows
            if self.selected > 0 && self.up_arrow.should_draw() {
                drawn |= self.up_arrow.draw(display, styles)?;
            }

            if self.selected < self.games.len() - 1 && self.down_arrow.should_draw() {
                drawn |= self.down_arrow.draw(display, styles)?;
            }
        }

        // Draw button hints
        if self.button_hints.should_draw() {
            drawn |= self.button_hints.draw(display, styles)?;
        }

        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        self.dirty
            || self.screenshot.should_draw()
            || self.game_name.should_draw()
            || self.counter_label.should_draw()
            || self.up_arrow.should_draw()
            || self.down_arrow.should_draw()
            || self.button_hints.should_draw()
    }

    fn set_should_draw(&mut self) {
        self.dirty = true;
        self.screenshot.set_should_draw();
        self.game_name.set_should_draw();
        self.counter_label.set_should_draw();
        self.up_arrow.set_should_draw();
        self.down_arrow.set_should_draw();
        self.button_hints.set_should_draw();
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        commands: Sender<Command>,
        _bubble: &mut VecDeque<Command>,
    ) -> Result<bool> {
        match event {
            KeyEvent::Pressed(Key::Up) | KeyEvent::Autorepeat(Key::Up) => {
                self.navigate_up()?;
                Ok(true)
            }
            KeyEvent::Pressed(Key::Down) | KeyEvent::Autorepeat(Key::Down) => {
                self.navigate_down()?;
                Ok(true)
            }
            KeyEvent::Pressed(Key::A) => {
                self.launch_game(commands).await?;
                Ok(true)
            }
            KeyEvent::Pressed(Key::X) => {
                // TODO: Implement search
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![]
    }

    fn bounding_box(&mut self, _styles: &Stylesheet) -> Rect {
        self.rect
    }

    fn set_position(&mut self, point: Point) {
        self.rect.x = point.x;
        self.rect.y = point.y;
    }
}
