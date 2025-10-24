use std::collections::HashMap;
use std::collections::VecDeque;

use anyhow::Result;
use async_trait::async_trait;
use common::command::Command;
use common::constants::RECENT_GAMES_LIMIT;
use common::database::Database;
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
use crate::view::entry_list::{EntryList, EntryListState};

pub type SearchResultsState = EntryListState<SearchResultsSort>;

/// Sort modes for search results
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SearchResultsSort {
    Relevance(String),    // Database order (default relevance)
    Alphabetical(String), // A-Z by game name
    LastPlayed(String),   // Most recently played first
    MostPlayed(String),   // Highest play count first
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
        // Search results don't use directories
        self.clone()
    }

    fn entries(
        &self,
        database: &Database,
        _console_mapper: &ConsoleMapper,
        _locale: &Locale,
    ) -> Result<Vec<Entry>> {
        let query = self.query();

        // Get search results from database
        let mut games = database.search(query, RECENT_GAMES_LIMIT)?;

        // Apply additional sorting if needed
        match self {
            SearchResultsSort::Relevance(_) => {
                // Database already returns in relevance order
            }
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

        // Convert to Entry::Game
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

/// Standalone search results view
///
/// Displays search results with sorting and allows:
/// - A: Launch selected game
/// - B: Go back to previous view
/// - Y: Cycle sort mode
/// - X: Edit search query
#[derive(Debug)]
pub struct SearchResultsView {
    rect: Rect,
    res: Resources,
    query: String,
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

        // Get result count first
        let entry_count = {
            let database = res.get::<Database>();
            let games = database.search(&query, RECENT_GAMES_LIMIT)?;
            games.len()
        };

        // Create entry list for games
        let sort = SearchResultsSort::Relevance(query.clone());
        let list = EntryList::new(
            Rect::new(x, y + 48, w, h - 48), // Leave space for header
            res.clone(),
            sort,
        )?;

        // Format result count text
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

        // Header showing search query
        let header = Label::new(
            Point::new(x + 12, y + 8),
            format!("Search: {}", query),
            Alignment::Left,
            Some(w - 24),
        );

        // Result count label
        let result_count = Label::new(
            Point::new(x + 12, y + 28),
            result_text,
            Alignment::Left,
            Some(w - 24),
        );

        // Button hints
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
                        locale.t("button-launch"),
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
                        locale.t("button-sort"),
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
            list,
            header,
            result_count,
            button_hints,
            search_view: SearchView::new(res),
        })
    }

    pub fn save(&self) -> SearchResultsState {
        self.list.save()
    }

    #[allow(dead_code)] // Reserved for future state restoration
    pub fn load_or_new(
        rect: Rect,
        res: Resources,
        state: Option<SearchResultsState>,
    ) -> Result<Self> {
        if let Some(state) = state {
            let query = state.sort.query().to_string();
            let list = EntryList::load(rect, res.clone(), state)?;

            // Recreate the view with the loaded list
            let mut view = Self::new(rect, res, query)?;
            view.list = list;
            Ok(view)
        } else {
            // Default to empty search
            Self::new(rect, res, String::new())
        }
    }

    #[allow(dead_code)] // May be useful for query-based operations
    pub fn query(&self) -> &str {
        &self.query
    }

    /// Update search query and reload results
    pub fn update_query(&mut self, new_query: String) -> Result<()> {
        self.query = new_query.clone();
        self.header.set_text(format!("🔍 {}", new_query));

        // Update list with new query and get result count
        let database = self.res.get::<Database>();
        let games = database.search(&new_query, RECENT_GAMES_LIMIT)?;
        let entry_count = games.len();

        let sort = SearchResultsSort::Relevance(new_query);
        self.list.sort(sort)?;

        // Update result count text
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
}

#[async_trait(?Send)]
impl View for SearchResultsView {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        let mut drawn = false;

        // Draw solid background to cover content behind
        // Leave space for button hints at bottom
        if self.should_draw() {
            let button_hint_height = ButtonIcon::diameter(styles) + 16; // Icon + padding
            let background_rect = Rectangle::new(
                embedded_graphics::prelude::Point::new(self.rect.x, self.rect.y),
                embedded_graphics::prelude::Size::new(
                    self.rect.w,
                    self.rect.h.saturating_sub(button_hint_height),
                ),
            );
            display.fill_solid(&background_rect, styles.background_color)?;
            drawn = true;
        }

        // Draw header and result count
        if self.header.should_draw() {
            drawn |= self.header.draw(display, styles)?;
        }
        if self.result_count.should_draw() {
            drawn |= self.result_count.draw(display, styles)?;
        }

        // Draw list
        if self.list.should_draw() {
            drawn |= self.list.draw(display, styles)?;
            if drawn {
                self.button_hints.set_should_draw();
            }
        }

        // Draw button hints
        drawn |= self.button_hints.should_draw() && self.button_hints.draw(display, styles)?;

        // Draw search overlay if editing query
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
        // If editing search query, let SearchView handle it
        if self.search_view.is_active() {
            if self
                .search_view
                .handle_key_event(event, commands.clone(), bubble)
                .await?
            {
                // Check if we got a new search query from the bubble
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

        // Handle our own keys
        match event {
            KeyEvent::Pressed(Key::B) => {
                // Go back to previous view - add to bubble so app can handle it
                bubble.push_back(Command::CloseView);
                Ok(true)
            }
            KeyEvent::Pressed(Key::X) => {
                // Edit search query
                self.search_view.activate_with_value(self.query.clone());
                commands.send(Command::Redraw).await?;
                Ok(true)
            }
            _ => {
                // Let list handle everything else (navigation, selection, sorting)
                self.list.handle_key_event(event, commands, bubble).await
            }
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
