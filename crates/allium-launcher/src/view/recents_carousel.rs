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
use common::view::{Image, ImageMode, Label, View};
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

        drop(styles);

        let mut carousel = Self {
            rect,
            res,
            games,
            selected,
            screenshot,
            game_name,
            counter_label,
            dirty: true,
        };

        carousel.update_current_game()?;

        Ok(carousel)
    }

    fn load_games(res: &Resources) -> Result<Vec<Game>> {
        let database = res.get::<Database>();
        let games = database.select_last_played(RECENT_GAMES_LIMIT)?;

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

    fn update_current_game(&mut self) -> Result<()> {
        if self.games.is_empty() {
            self.screenshot.set_path(None);
            self.game_name.set_text(String::new());
            self.counter_label.set_text(String::new());
            return Ok(());
        }

        let game = &self.games[self.selected];

        let screenshot_path = if self.selected == 0 {
            Self::find_screenshot_for_game_with_retry(&game.path)
        } else {
            Self::find_screenshot_for_game(&game.path)
        };
        
        self.screenshot.set_path(screenshot_path);
        self.game_name.set_text(game.name.clone());
        self.counter_label
            .set_text(format!("{}/{}", self.selected + 1, self.games.len()));

        self.dirty = true;
        Ok(())
    }

    fn find_screenshot_for_game_with_retry(game_path: &PathBuf) -> Option<PathBuf> {
        use std::thread;
        use std::time::Duration;
        
        for attempt in 0..5 {
            if attempt > 0 {
                let delay_ms = attempt * 50;
                thread::sleep(Duration::from_millis(delay_ms as u64));
            }
            
            if let Some(screenshot) = Self::find_screenshot_for_game(game_path) {
                return Some(screenshot);
            }
        }
        
        None
    }

    fn find_screenshot_for_game(game_path: &PathBuf) -> Option<PathBuf> {
        use std::fs;
        use std::io::{BufRead, BufReader};
        
        let manifest_path = ALLIUM_SCREENSHOTS_DIR.join("manifest.txt");
        
        if let Ok(file) = fs::File::open(&manifest_path) {
            let reader = BufReader::new(file);
            let mut matching_entries: Vec<(u64, PathBuf)> = Vec::new();
            
            let game_path_str = game_path.to_string_lossy();
            
            for line in reader.lines().filter_map(|l| l.ok()) {
                let parts: Vec<&str> = line.split('|').collect();
                if parts.len() >= 3 {
                    if let Ok(timestamp) = parts[0].parse::<u64>() {
                        let filename = parts[1];
                        let manifest_game_path = parts[2].trim();
                        
                        if manifest_game_path == game_path_str {
                            let screenshot_path = ALLIUM_SCREENSHOTS_DIR.join(filename);
                            if screenshot_path.exists() {
                                matching_entries.push((timestamp, screenshot_path));
                            }
                        }
                    }
                }
            }
            
            matching_entries.sort_by(|a, b| b.0.cmp(&a.0));
            return matching_entries.first().map(|(_, path)| path.clone());
        }
        
        None
    }

    pub fn save(&self) -> RecentsCarouselState {
        RecentsCarouselState { selected: 0 }
    }

    pub fn reset_selection(&mut self) -> Result<()> {
        if self.selected != 0 {
            self.selected = 0;
            self.update_current_game()?;
        }
        Ok(())
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
        }

        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        self.dirty
            || self.screenshot.should_draw()
            || self.game_name.should_draw()
            || self.counter_label.should_draw()
    }

    fn set_should_draw(&mut self) {
        self.dirty = true;
        self.screenshot.set_should_draw();
        self.game_name.set_should_draw();
        self.counter_label.set_should_draw();
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
