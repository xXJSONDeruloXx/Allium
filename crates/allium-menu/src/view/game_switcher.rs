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

    fn draw_game_card(
        &self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
        game: &GameHistoryEntry,
        x: i32,
        y: i32,
        width: u32,
        height: u32,
        is_selected: bool,
    ) -> Result<()> {
        let card_rect = Rectangle::new(
            Point::new(x, y).into(),
            Size::new(width, height),
        );

        // Draw card background
        let bg_color = if is_selected {
            styles.highlight_color
        } else {
            styles.disabled_color
        };

        RoundedRectangle::with_equal_corners(
            card_rect,
            Size::new_equal(8),
        )
        .into_styled(PrimitiveStyle::with_fill(bg_color))
        .draw(display)?;

        // TODO: Draw screenshot if available
        // For now, just draw a placeholder rectangle
        let screenshot_rect = Rectangle::new(
            Point::new(x + 8, y + 8).into(),
            Size::new(width - 16, height - 50),
        );

        RoundedRectangle::with_equal_corners(
            screenshot_rect,
            Size::new_equal(4),
        )
        .into_styled(PrimitiveStyle::with_fill(styles.background_color))
        .draw(display)?;

        // Draw game name
        let text_style = FontTextStyleBuilder::new(styles.ui_font.font())
            .font_fallback(styles.cjk_font.font())
            .font_size(styles.ui_font.size)
            .background_color(bg_color)
            .text_color(if is_selected {
                styles.background_color
            } else {
                styles.foreground_color
            })
            .build();

        let name_y = y + (height - 30) as i32;
        Text::with_alignment(
            &game.name,
            Point::new(x + (width / 2) as i32, name_y).into(),
            text_style,
            Alignment::Center.into(),
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
                // Draw carousel of game cards
                let card_width = 200;
                let card_height = 180;
                let card_spacing = 20;
                let center_x = self.rect.x + (self.rect.w / 2) as i32;
                let center_y = self.rect.y + (self.rect.h / 2) as i32 - 30;

                // Draw up to 3 cards: prev, current, next
                let num_games = self.games.len();
                
                // Previous card (if exists)
                if num_games > 1 {
                    let prev_idx = if self.selected == 0 {
                        num_games - 1
                    } else {
                        self.selected - 1
                    };
                    let prev_x = center_x - card_width as i32 - card_spacing;
                    self.draw_game_card(
                        display,
                        styles,
                        &self.games[prev_idx],
                        prev_x,
                        center_y,
                        card_width,
                        card_height,
                        false,
                    )?;
                }

                // Current card (selected)
                let current_x = center_x - (card_width / 2) as i32;
                self.draw_game_card(
                    display,
                    styles,
                    &self.games[self.selected],
                    current_x,
                    center_y - 10, // Slightly higher to emphasize
                    card_width,
                    card_height + 20,
                    true,
                )?;

                // Next card (if exists)
                if num_games > 1 {
                    let next_idx = if self.selected == num_games - 1 {
                        0
                    } else {
                        self.selected + 1
                    };
                    let next_x = center_x + card_width as i32 / 2 + card_spacing;
                    self.draw_game_card(
                        display,
                        styles,
                        &self.games[next_idx],
                        next_x,
                        center_y,
                        card_width,
                        card_height,
                        false,
                    )?;
                }
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

        // Create new GameInfo for the selected game BEFORE quitting
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
        debug!("Saving new game info to disk (THIS WILL BE LOADED BY ALLIUMD)");
        new_game_info.save()?;

        // NOW quit RetroArch - alliumd will detect the quit and launch the new game
        debug!("Sending quit command to RetroArch");
        RetroArchCommand::Quit.send().await?;
        
        // Exit the menu - when alliumd sees RetroArch quit, it will spawn_main()
        // which will load our saved GameInfo and launch the new game
        debug!("Exiting menu - alliumd will launch the new game");
        commands.send(Command::Exit).await?;
        
        debug!("=== GAME SWITCH END ===");

        Ok(())
    }
}
