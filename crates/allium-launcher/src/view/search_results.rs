use std::collections::HashMap;
use std::collections::VecDeque;

use anyhow::Result;
use async_trait::async_trait;
use common::command::Command;
use common::constants::RECENT_GAMES_LIMIT;
use common::database::Database;
use common::display::Display;
use common::geom::{Alignment, Point, Rect};
use common::locale::{Locale, LocaleFluentValue};
use common::platform::{DefaultPlatform, Key, KeyEvent, Platform};
use common::resources::Resources;
use common::stylesheet::Stylesheet;
use common::view::{ButtonHint, ButtonIcon, Label, Row, SearchView, View};
use embedded_graphics::prelude::*;
use embedded_graphics::primitives::Rectangle;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc::Sender;

use crate::consoles::ConsoleMapper;
use crate::entry::directory::Directory;
use crate::entry::game::Game;
use crate::entry::lazy_image::LazyImage;
use crate::entry::{Entry, Sort};
use crate::view::entry_list::EntryList;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchResultsSort {
    Relevance(String),
    Alphabetical(String),
    LastPlayed(String),
    MostPlayed(String),
}

impl SearchResultsSort {
    fn query(&self) -> &str {
        match self {
            SearchResultsSort::Relevance(q) => q,
            SearchResultsSort::Alphabetical(q) => q,
            SearchResultsSort::LastPlayed(q) => q,
            SearchResultsSort::MostPlayed(q) => q,
        }
    }
}

impl Sort for SearchResultsSort {
    const HAS_BUTTON_HINTS: bool = true;

    fn button_hint(&self, locale: &Locale) -> String {
        match self {
            SearchResultsSort::Relevance(_) => locale.t("sort-relevance"),
            SearchResultsSort::Alphabetical(_) => locale.t("sort-alphabetical"),
            SearchResultsSort::LastPlayed(_) => locale.t("sort-last-played"),
            SearchResultsSort::MostPlayed(_) => locale.t("sort-most-played"),
        }
    }

    fn next(&self) -> Self {
        let query = self.query().to_string();
        match self {
            SearchResultsSort::Relevance(_) => SearchResultsSort::Alphabetical(query),
            SearchResultsSort::Alphabetical(_) => SearchResultsSort::LastPlayed(query),
            SearchResultsSort::LastPlayed(_) => SearchResultsSort::MostPlayed(query),
            SearchResultsSort::MostPlayed(_) => SearchResultsSort::Relevance(query),
        }
    }

    fn with_directory(&self, _directory: Directory) -> Self {
        self.clone()
    }

    fn entries(
        &self,
        database: &Database,
        _console_mapper: &ConsoleMapper,
        _locale: &Locale,
    ) -> Result<Vec<Entry>> {
        let query = self.query();

        let mut games = database.search(query, RECENT_GAMES_LIMIT)?;

        match self {
            SearchResultsSort::Relevance(_) => {}
            SearchResultsSort::Alphabetical(_) => {
                games.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
            }
            SearchResultsSort::LastPlayed(_) => {
                games.sort_by(|a, b| b.last_played.cmp(&a.last_played));
            }
            SearchResultsSort::MostPlayed(_) => {
                games.sort_by(|a, b| b.play_count.cmp(&a.play_count));
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

                let full_name = game.name.clone();
                let image = LazyImage::from_path(&game.path, game.image);

                Entry::Game(Game {
                    name: game.name,
                    full_name,
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
                    screenshot_path: game.screenshot_path,
                })
            })
            .collect())
    }

    fn preserve_selection(&self) -> bool {
        false
    }
}

#[derive(Debug)]
pub struct SearchResultsView {
    rect: Rect,
    res: Resources,
    query: String,
    current_sort: SearchResultsSort,
    list: EntryList<SearchResultsSort>,
    header: Label<String>,
    result_count: Label<String>,
    button_hints: Row<ButtonHint<String>>,
    search_view: SearchView,
}

impl SearchResultsView {
    pub fn new(rect: Rect, res: Resources, query: String) -> Result<Self> {
        let Rect { x, y, w, h } = rect;
        let styles = res.get::<Stylesheet>();

        let entry_count = {
            let database = res.get::<Database>();
            let games = database.search(&query, RECENT_GAMES_LIMIT)?;
            games.len()
        };

        let sort = SearchResultsSort::Relevance(query.clone());
        let list = EntryList::new(Rect::new(x, y + 72, w, h - 72), res.clone(), sort.clone())?;

        let result_text = {
            let locale = res.get::<Locale>();
            if entry_count == 0 {
                locale.t("no-search-results")
            } else if entry_count == 1 {
                locale.t("one-search-result")
            } else {
                let mut map = HashMap::new();
                map.insert(
                    "count".into(),
                    LocaleFluentValue::from(entry_count.to_string()),
                );
                locale.ta("n-search-results", &map)
            }
        };

        let header = Label::new(
            Point::new(x + 12, y + 8),
            format!("Search: {}", query),
            Alignment::Left,
            Some(w - 24),
        );

        let result_count = Label::new(
            Point::new(x + 12, y + 36),
            result_text,
            Alignment::Left,
            Some(w - 24),
        );

        let button_hints = Row::new(
            Point::new(
                x + w as i32 - 12,
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
                        Alignment::Right,
                    ),
                    ButtonHint::new(
                        res.clone(),
                        Point::zero(),
                        Key::B,
                        locale.t("button-back"),
                        Alignment::Right,
                    ),
                    ButtonHint::new(
                        res.clone(),
                        Point::zero(),
                        Key::Y,
                        sort.button_hint(&locale),
                        Alignment::Right,
                    ),
                    ButtonHint::new(
                        res.clone(),
                        Point::zero(),
                        Key::X,
                        locale.t("button-edit-search"),
                        Alignment::Right,
                    ),
                ]
            },
            Alignment::Right,
            12,
        );

        drop(styles);

        Ok(Self {
            rect,
            res: res.clone(),
            query,
            current_sort: sort,
            list,
            header,
            result_count,
            button_hints,
            search_view: SearchView::new(res),
        })
    }

    pub fn update_query(&mut self, new_query: String) -> Result<()> {
        self.query = new_query.clone();
        self.header.set_text(format!("🔍 {}", new_query));

        let entry_count = {
            let database = self.res.get::<Database>();
            let games = database.search(&new_query, RECENT_GAMES_LIMIT)?;
            games.len()
        };

        let sort = SearchResultsSort::Relevance(new_query);
        self.current_sort = sort.clone();
        self.list.sort(sort)?;
        self.update_sort_button_hint();

        let result_text = if entry_count == 0 {
            self.res.get::<Locale>().t("no-search-results")
        } else if entry_count == 1 {
            self.res.get::<Locale>().t("one-search-result")
        } else {
            let mut map = HashMap::new();
            map.insert(
                "count".into(),
                LocaleFluentValue::from(entry_count.to_string()),
            );
            self.res.get::<Locale>().ta("n-search-results", &map)
        };
        self.result_count.set_text(result_text);

        Ok(())
    }

    fn update_sort_button_hint(&mut self) {
        let locale = self.res.get::<Locale>();
        let sort_text = self.current_sort.button_hint(&locale);
        self.button_hints.get_mut(2).unwrap().set_text(sort_text);
    }
}

#[async_trait(?Send)]
impl View for SearchResultsView {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        let mut drawn = false;

        let needs_full_redraw = self.header.should_draw() || self.result_count.should_draw();

        if needs_full_redraw {
            let button_hint_height = ButtonIcon::diameter(styles) + 16;
            let background_rect = Rectangle::new(
                embedded_graphics::prelude::Point::new(self.rect.x, self.rect.y),
                embedded_graphics::prelude::Size::new(
                    self.rect.w,
                    self.rect.h.saturating_sub(button_hint_height),
                ),
            );
            display.fill_solid(&background_rect, styles.background_color)?;
            drawn = true;

            self.header.set_should_draw();
            self.result_count.set_should_draw();
            self.list.set_should_draw();
            self.button_hints.set_should_draw();
        }

        drawn |= self.header.should_draw() && self.header.draw(display, styles)?;
        drawn |= self.result_count.should_draw() && self.result_count.draw(display, styles)?;

        drawn |= self.list.should_draw() && self.list.draw(display, styles)?;

        if self.button_hints.should_draw() {
            display.load(Rect::new(
                0,
                display.size().height as i32 - 48,
                display.size().width,
                48,
            ))?;
            self.button_hints.set_should_draw();
            drawn |= self.button_hints.draw(display, styles)?;
        }

        if self.search_view.is_active() {
            display.load(self.rect)?;
            self.search_view.set_should_draw();
        }
        drawn |= self.search_view.draw(display, styles)?;

        Ok(drawn)
    }

    fn should_draw(&self) -> bool {
        self.header.should_draw()
            || self.result_count.should_draw()
            || self.list.should_draw()
            || self.button_hints.should_draw()
            || self.search_view.should_draw()
    }

    fn set_should_draw(&mut self) {
        self.header.set_should_draw();
        self.result_count.set_should_draw();
        self.list.set_should_draw();
        self.button_hints.set_should_draw();
        self.search_view.set_should_draw();
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        commands: Sender<Command>,
        bubble: &mut VecDeque<Command>,
    ) -> Result<bool> {
        if self.search_view.is_active() {
            if self
                .search_view
                .handle_key_event(event, commands.clone(), bubble)
                .await?
            {
                for cmd in bubble.iter() {
                    if let Command::Search(new_query) = cmd {
                        self.update_query(new_query.clone())?;
                        commands.send(Command::Redraw).await?;
                        break;
                    }
                }
                return Ok(true);
            }
        }

        match event {
            KeyEvent::Pressed(Key::B) => {
                bubble.push_back(Command::CloseView);
                Ok(true)
            }
            KeyEvent::Pressed(Key::X) => {
                self.search_view.activate_with_value(self.query.clone());
                commands.send(Command::Redraw).await?;
                Ok(true)
            }
            KeyEvent::Pressed(Key::Y) => {
                if self.list.handle_key_event(event, commands, bubble).await? {
                    self.current_sort = self.current_sort.next();
                    self.update_sort_button_hint();
                    self.button_hints.set_should_draw();
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
            _ => self.list.handle_key_event(event, commands, bubble).await,
        }
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![&self.list]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![&mut self.list]
    }

    fn bounding_box(&mut self, _styles: &Stylesheet) -> Rect {
        self.rect
    }

    fn set_position(&mut self, _point: Point) {
        unimplemented!()
    }
}
