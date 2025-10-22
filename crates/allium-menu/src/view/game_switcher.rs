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
use log::debug;
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
        debug!("GameSwitcher::new - Initializing");
        
        let current_game = res.get::<GameInfo>();
        let current_game_path = Some(current_game.path.clone());
        drop(current_game);

        let db = res.get::<Database>().clone();
        let history = GameHistory::new(db);
        let games = history.get_recent_games(current_game_path.as_ref(), 9)?;
        
        debug!("Found {} games in history", games.len());

        let locale = res.get::<Locale>();
        let styles = res.get::<Stylesheet>();

        let button_hints = Row::new(
            Point::new(
                rect.x + rect.w as i32 - 12,
                rect.y + rect.h as i32 - ButtonIcon::diameter(&styles) as i32 - 8,
            ),
            vec![
                ButtonHint::new(res.clone(), Point::zero(), Key::A, locale.t("button-select"), Alignment::Right),
                ButtonHint::new(res.clone(), Point::zero(), Key::B, locale.t("button-back"), Alignment::Right),
            ],
            Alignment::Right,
            12,
        );

        drop(locale);
        drop(styles);

        Ok(Self { rect, res, games, selected: 0, button_hints, dirty: true })
    }
}

#[async_trait(?Send)]
impl View for GameSwitcher {
    fn draw(&mut self, display: &mut <DefaultPlatform as Platform>::Display, styles: &Stylesheet) -> Result<bool> {
        let mut drawn = false;

        if self.dirty {
            RoundedRectangle::with_equal_corners(
                Rectangle::new(
                    Point::new(self.rect.x + 12, self.rect.y + 12).into(),
                    Size::new(self.rect.w - 24, self.rect.h - 24 - ButtonIcon::diameter(styles)),
                ),
                Size::new_equal(8),
            )
            .into_styled(PrimitiveStyle::with_fill(styles.background_color))
            .draw(display)?;

            if self.games.is_empty() {
                let text_style = FontTextStyleBuilder::new(styles.ui_font.font())
                    .font_fallback(styles.cjk_font.font())
                    .font_size(styles.ui_font.size)
                    .background_color(styles.background_color)
                    .text_color(styles.foreground_color)
                    .build();

                Text::with_alignment(
                    "No games in history",
                    Point::new(self.rect.x + (self.rect.w / 2) as i32, self.rect.y + (self.rect.h / 2) as i32).into(),
                    text_style,
                    Alignment::Center.into(),
                )
                .draw(display)?;
            } else {
                let game = &self.games[self.selected];
                let text_style = FontTextStyleBuilder::new(styles.ui_font.font())
                    .font_fallback(styles.cjk_font.font())
                    .font_size(styles.ui_font.size)
                    .background_color(styles.background_color)
                    .text_color(styles.foreground_color)
                    .build();

                let y_pos = self.rect.y + (self.rect.h / 2) as i32;
                Text::with_alignment(
                    &format!("{} ({}/{})", game.name, self.selected + 1, self.games.len()),
                    Point::new(self.rect.x + (self.rect.w / 2) as i32, y_pos).into(),
                    text_style,
                    Alignment::Center.into(),
                )
                .draw(display)?;
            }

            self.dirty = false;
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

    async fn handle_key_event(&mut self, event: KeyEvent, commands: Sender<Command>, _bubble: &mut VecDeque<Command>) -> Result<bool> {
        match event {
            KeyEvent::Pressed(Key::Left) | KeyEvent::Autorepeat(Key::Left) => {
                if !self.games.is_empty() {
                    self.selected = if self.selected == 0 { self.games.len() - 1 } else { self.selected - 1 };
                    self.dirty = true;
                }
                Ok(true)
            }
            KeyEvent::Pressed(Key::Right) | KeyEvent::Autorepeat(Key::Right) => {
                if !self.games.is_empty() {
                    self.selected = (self.selected + 1) % self.games.len();
                    self.dirty = true;
                }
                Ok(true)
            }
            KeyEvent::Pressed(Key::A) => {
                if !self.games.is_empty() {
                    self.switch_to_game(commands).await?;
                }
                Ok(true)
            }
            KeyEvent::Pressed(Key::B) | KeyEvent::Pressed(Key::Menu) => {
                commands.send(Command::CloseView).await?;
                Ok(true)
            }
            _ => Ok(false),
        }
    }

    fn children(&self) -> Vec<&dyn View> { vec![] }
    fn children_mut(&mut self) -> Vec<&mut dyn View> { vec![] }
    fn bounding_box(&mut self, _styles: &Stylesheet) -> Rect { self.rect }
    fn set_position(&mut self, point: Point) { self.rect.x = point.x; self.rect.y = point.y; }
}

impl GameSwitcher {
    async fn switch_to_game(&self, commands: Sender<Command>) -> Result<()> {
        let game = &self.games[self.selected];
        debug!("Switching to: {}", game.name);

        let current_game = self.res.get::<GameInfo>();
        if current_game.has_menu {
            debug!("Auto-saving current game");
            RetroArchCommand::SaveStateSlot(0).send().await?;
            tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
        }
        drop(current_game);

        debug!("Quitting current game");
        RetroArchCommand::Quit.send().await?;
        tokio::time::sleep(tokio::time::Duration::from_millis(1000)).await;

        // Create and save the new game info
        let new_game_info = GameInfo::new(
            game.name.clone(),
            game.path.clone(),
            game.core.clone(),
            None,
            game.command.clone(),
            game.args.clone(),
            game.has_menu,
            game.needs_swap,
        );
        
        debug!("Saving new game info: {:?}", new_game_info.name);
        new_game_info.save()?;

        // Use Command::Exec to replace the current process with the new game
        // This is the same way the launcher launches games
        let cmd = new_game_info.command();
        debug!("Executing game command");
        commands.send(Command::Exec(cmd)).await?;
        
        Ok(())
    }
}
