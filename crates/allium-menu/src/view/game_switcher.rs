use std::collections::VecDeque;

use anyhow::Result;
use async_trait::async_trait;
use common::command::Command;
use common::database::Database;
use common::display::font::FontTextStyleBuilder;
use common::game_history::{GameHistory, GameHistoryEntry};
use common::game_info::GameInfo;
use common::geom::{Alignment, Point, Rect};
use common::locale::Locale;
use common::platform::{DefaultPlatform, Key, KeyEvent, Platform};
use common::resources::Resources;
use common::retroarch::RetroArchCommand;
use common::stylesheet::Stylesheet;
use common::view::{ButtonHint, ButtonIcon, Row, View};
use embedded_graphics::Drawable;
use embedded_graphics::prelude::Size;
use embedded_graphics::primitives::{Primitive, PrimitiveStyle, Rectangle, RoundedRectangle};
use embedded_graphics::text::Text;
use log::{debug, trace, warn};
use tokio::sync::mpsc::Sender;

pub struct GameSwitcher {
    rect: Rect,
    res: Resources,
    games: Vec<GameHistoryEntry>,
    selected: usize,
    button_hints: Row<ButtonHint<String>>,
    dirty: bool,
}

impl GameSwitcher {
    pub fn new(rect: Rect, res: Resources) -> Result<Self> {
        debug!("GameSwitcher::new - Initializing game switcher");
        
        let current_game = res.get::<GameInfo>();
        let current_game_path = Some(current_game.path.clone());
        debug!("Current game: {} at {:?}", current_game.name, current_game.path);
        drop(current_game);

        let db = res.get::<Database>().clone();
        let history = GameHistory::new(db);
        
        // Get recent games excluding the current one
        let games = history.get_recent_games(current_game_path.as_ref(), 9)?;
        
        debug!("Found {} games in history", games.len());
        for (i, game) in games.iter().enumerate() {
            debug!("  [{}] {} - {:?}", i, game.name, game.path);
        }
        
        if games.is_empty() {
            warn!("No games in history to switch to");
        }

        let locale = res.get::<Locale>();
        let styles = res.get::<Stylesheet>();

        let button_hints = Row::new(
            Point::new(
                rect.x + rect.w as i32 - 12,
                rect.y + rect.h as i32 - ButtonIcon::diameter(&styles) as i32 - 8,
            ),
            vec![
                ButtonHint::new(
                    res.clone(),
                    Point::zero(),
                    Key::A,
                    locale.t("button-select"),
                    Alignment::Right,
                ),
                ButtonHint::new(
                    res.clone(),
                    Point::zero(),
                    Key::B,
                    locale.t("button-back"),
                    Alignment::Right,
                ),
            ],
            Alignment::Right,
            12,
        );

        drop(locale);
        drop(styles);

        Ok(Self {
            rect,
            res,
            games,
            selected: 0,
            button_hints,
            dirty: true,
        })
    }

    fn draw_screenshot_area(
        &self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
        area_rect: Rect,
    ) -> Result<()> {
        // TODO: Load actual screenshot if available
        // For now, draw a placeholder rectangle
        let placeholder_rect = Rectangle::new(
            Point::new(area_rect.x, area_rect.y).into(),
            Size::new(area_rect.w, area_rect.h),
        );

        RoundedRectangle::with_equal_corners(
            placeholder_rect,
            Size::new_equal(8),
        )
        .into_styled(PrimitiveStyle::with_fill(styles.disabled_color))
        .draw(display)?;

        // Draw "No Screenshot" text
        let text_style = FontTextStyleBuilder::new(styles.ui_font.font())
            .font_fallback(styles.cjk_font.font())
            .font_size(styles.ui_font.size)
            .background_color(styles.disabled_color)
            .text_color(styles.foreground_color)
            .build();

        Text::with_alignment(
            "No Screenshot",
            Point::new(
                area_rect.x + (area_rect.w / 2) as i32,
                area_rect.y + (area_rect.h / 2) as i32,
            )
            .into(),
            text_style,
            Alignment::Center.into(),
        )
        .draw(display)?;

        Ok(())
    }

    fn draw_game_name_bar(
        &self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
        bar_rect: Rect,
        game: &GameHistoryEntry,
    ) -> Result<()> {
        // Draw semi-transparent background bar
        let bar_bg = Rectangle::new(
            Point::new(bar_rect.x, bar_rect.y).into(),
            Size::new(bar_rect.w, bar_rect.h),
        );

        RoundedRectangle::with_equal_corners(
            bar_bg,
            Size::new_equal(4),
        )
        .into_styled(PrimitiveStyle::with_fill(styles.highlight_color))
        .draw(display)?;

        // Draw left arrow if not at start
        if self.selected > 0 {
            let arrow_style = FontTextStyleBuilder::new(styles.ui_font.font())
                .font_fallback(styles.cjk_font.font())
                .font_size(styles.ui_font.size)
                .background_color(styles.highlight_color)
                .text_color(styles.background_color)
                .build();

            Text::with_alignment(
                "<",
                Point::new(bar_rect.x + 20, bar_rect.y + (bar_rect.h / 2) as i32).into(),
                arrow_style,
                Alignment::Left.into(),
            )
            .draw(display)?;
        }

        // Draw game name (centered)
        let name_style = FontTextStyleBuilder::new(styles.ui_font.font())
            .font_fallback(styles.cjk_font.font())
            .font_size(styles.ui_font.size)
            .background_color(styles.highlight_color)
            .text_color(styles.background_color)
            .build();

        Text::with_alignment(
            &game.name,
            Point::new(
                bar_rect.x + (bar_rect.w / 2) as i32,
                bar_rect.y + (bar_rect.h / 2) as i32,
            )
            .into(),
            name_style,
            Alignment::Center.into(),
        )
        .draw(display)?;

        // Draw right arrow if not at end
        if self.selected < self.games.len() - 1 {
            let arrow_style = FontTextStyleBuilder::new(styles.ui_font.font())
                .font_fallback(styles.cjk_font.font())
                .font_size(styles.ui_font.size)
                .background_color(styles.highlight_color)
                .text_color(styles.background_color)
                .build();

            Text::with_alignment(
                ">",
                Point::new(bar_rect.x + bar_rect.w as i32 - 20, bar_rect.y + (bar_rect.h / 2) as i32).into(),
                arrow_style,
                Alignment::Right.into(),
            )
            .draw(display)?;
        }

        // Draw game counter (e.g., "3/10") on the right
        let counter_text = format!("{}/{}", self.selected + 1, self.games.len());
        let counter_style = FontTextStyleBuilder::new(styles.ui_font.font())
            .font_size(styles.ui_font.size - 4) // Slightly smaller
            .background_color(styles.highlight_color)
            .text_color(styles.background_color)
            .build();

        Text::with_alignment(
            &counter_text,
            Point::new(
                bar_rect.x + bar_rect.w as i32 - 50,
                bar_rect.y + (bar_rect.h / 2) as i32,
            )
            .into(),
            counter_style,
            Alignment::Right.into(),
        )
        .draw(display)?;

        Ok(())
    }
}

#[async_trait(?Send)]
impl View for GameSwitcher {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        let mut drawn = false;

        if self.dirty {
            // Draw background
            RoundedRectangle::with_equal_corners(
                Rectangle::new(
                    Point::new(self.rect.x + 12, self.rect.y + 12).into(),
                    Size::new(
                        self.rect.w - 24,
                        self.rect.h - 24 - ButtonIcon::diameter(styles),
                    ),
                ),
                Size::new_equal(8),
            )
            .into_styled(PrimitiveStyle::with_fill(styles.background_color))
            .draw(display)?;

            if self.games.is_empty() {
                // Show "no games" message
                let text_style = FontTextStyleBuilder::new(styles.ui_font.font())
                    .font_fallback(styles.cjk_font.font())
                    .font_size(styles.ui_font.size)
                    .background_color(styles.background_color)
                    .text_color(styles.foreground_color)
                    .build();

                Text::with_alignment(
                    "No games in history",
                    Point::new(
                        self.rect.x + (self.rect.w / 2) as i32,
                        self.rect.y + (self.rect.h / 2) as i32,
                    )
                    .into(),
                    text_style,
                    Alignment::Center.into(),
                )
                .draw(display)?;
            } else {
                // Single-screenshot layout with name bar at bottom
                
                // Calculate layout areas
                let padding = 12i32;
                let button_hints_height = ButtonIcon::diameter(styles) as i32 + 16;
                let name_bar_height = 60u32;
                
                // Screenshot area (fullscreen, centered)
                let screenshot_area = Rect {
                    x: self.rect.x + padding * 2,
                    y: self.rect.y + padding * 2,
                    w: (self.rect.w as i32 - padding * 4) as u32,
                    h: (self.rect.h as i32 - padding * 4 - button_hints_height - name_bar_height as i32 - padding) as u32,
                };

                // Name bar area (at bottom, above button hints)
                let name_bar = Rect {
                    x: self.rect.x + padding * 2,
                    y: screenshot_area.y + screenshot_area.h as i32 + padding,
                    w: (self.rect.w as i32 - padding * 4) as u32,
                    h: name_bar_height,
                };

                // Draw fullscreen screenshot (or placeholder)
                self.draw_screenshot_area(display, styles, screenshot_area)?;

                // Draw game name bar with arrows and counter
                let current_game = &self.games[self.selected];
                self.draw_game_name_bar(display, styles, name_bar, current_game)?;
            }

            self.dirty = false;
            trace!("drawing game switcher");
            drawn = true;
        }

        drawn |= self.button_hints.draw(display, styles)?;

        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        self.dirty || self.button_hints.should_draw()
    }

    fn set_should_draw(&mut self) {
        self.dirty = true;
        self.button_hints.set_should_draw();
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        commands: Sender<Command>,
        _bubble: &mut VecDeque<Command>,
    ) -> Result<bool> {
        debug!("GameSwitcher::handle_key_event - received event: {:?}", event);
        
        match event {
            KeyEvent::Pressed(Key::Left) | KeyEvent::Autorepeat(Key::Left) => {
                if !self.games.is_empty() {
                    self.selected = if self.selected == 0 {
                        self.games.len() - 1
                    } else {
                        self.selected - 1
                    };
                    debug!("Navigate left - selected index: {}", self.selected);
                    self.dirty = true;
                }
                Ok(true)
            }
            KeyEvent::Pressed(Key::Right) | KeyEvent::Autorepeat(Key::Right) => {
                if !self.games.is_empty() {
                    self.selected = (self.selected + 1) % self.games.len();
                    debug!("Navigate right - selected index: {}", self.selected);
                    self.dirty = true;
                }
                Ok(true)
            }
            KeyEvent::Pressed(Key::A) => {
                debug!("A button pressed - attempting to switch game");
                if !self.games.is_empty() {
                    self.switch_to_game(commands).await?;
                } else {
                    warn!("Cannot switch - no games in history");
                }
                Ok(true)
            }
            KeyEvent::Pressed(Key::B) => {
                debug!("B button pressed - closing game switcher");
                // Send CloseView command to parent (ingame menu)
                commands.send(Command::CloseView).await?;
                Ok(true)
            }
            KeyEvent::Pressed(Key::Menu) => {
                debug!("Menu button pressed - closing game switcher");
                // Also close on Menu button
                commands.send(Command::CloseView).await?;
                Ok(true)
            }
            _ => {
                debug!("Unhandled key event: {:?}", event);
                Ok(false)
            }
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

impl GameSwitcher {
    async fn switch_to_game(&self, commands: Sender<Command>) -> Result<()> {
        let game = &self.games[self.selected];
        
        debug!("=== GAME SWITCH START ===");
        debug!("Switching to game: {}", game.name);
        debug!("Game path: {:?}", game.path);
        debug!("Core: {}", game.core);

        // Get current game's RetroArch state slot
        let current_game = self.res.get::<GameInfo>();
        let slot = 0; // Default to slot 0, could be made configurable
        
        // Auto-save current game state if it's a RetroArch game
        if current_game.has_menu {
            debug!("Current game has menu - auto-saving to slot {}", slot);
            RetroArchCommand::SaveStateSlot(slot).send().await?;
            
            // Give RetroArch time to save
            debug!("Waiting for save to complete...");
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        } else {
            debug!("Current game does not have menu - skipping auto-save");
        }
        drop(current_game);

        // Quit RetroArch first
        debug!("Sending quit command to RetroArch");
        RetroArchCommand::Quit.send().await?;
        
        // Give RetroArch time to quit
        debug!("Waiting for RetroArch to quit...");
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        // Create new GameInfo for the selected game
        let new_game_info = GameInfo::new(
            game.name.clone(),
            game.path.clone(),
            game.core.clone(),
            None, // Image will be loaded by launcher
            game.command.clone(),
            game.args.clone(),
            game.has_menu,
            game.needs_swap,
        );

        debug!("Created new GameInfo: {:?}", new_game_info);
        debug!("Saving new game info to disk");
        new_game_info.save()?;

        // Launch the game directly by spawning the process
        debug!("Spawning new game process directly");
        let mut cmd = new_game_info.command();
        match cmd.spawn() {
            Ok(_) => {
                debug!("Game process spawned successfully");
            }
            Err(e) => {
                warn!("Failed to spawn game process: {}", e);
            }
        }
        
        // Now exit the menu - the game should be running
        debug!("Exiting menu");
        commands.send(Command::Exit).await?;
        
        debug!("=== GAME SWITCH END ===");

        Ok(())
    }
}
