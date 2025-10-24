use std::collections::VecDeque;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use tokio::sync::mpsc::Sender;

use crate::command::{Command, Value};
use crate::database::Database;
use crate::geom::{Point, Rect};
use crate::locale::Locale;
use crate::platform::{DefaultPlatform, KeyEvent, Platform};
use crate::resources::Resources;
use crate::stylesheet::Stylesheet;
use crate::view::{Keyboard, View};

/// State of the search view
#[derive(Debug, Clone, PartialEq)]
pub enum SearchState {
    /// Search is not active
    Inactive,
    /// Keyboard is visible and user is typing
    Active,
    /// Query has been submitted, waiting for parent to handle results
    Searching,
}

/// A reusable search view that overlays any parent view
///
/// This view manages keyboard input for search queries and emits search commands
/// without knowing about the specific context (Recents, Games, etc.). Parent views
/// compose with SearchView and handle the actual search results display.
///
/// # Example
///
/// ```ignore
/// pub struct MyView {
///     search_view: SearchView,
///     // ... other fields
/// }
///
/// impl MyView {
///     pub fn start_search(&mut self) {
///         self.search_view.activate();
///     }
/// }
/// ```
#[derive(Debug)]
pub struct SearchView {
    res: Resources,
    keyboard: Option<Keyboard>,
    state: SearchState,
}

impl SearchView {
    /// Create a new inactive search view
    pub fn new(res: Resources) -> Self {
        Self {
            res,
            keyboard: None,
            state: SearchState::Inactive,
        }
    }

    /// Activate search and show keyboard
    pub fn activate(&mut self) {
        self.state = SearchState::Active;
        self.keyboard = Some(Keyboard::new(self.res.clone(), String::new(), false));
    }

    /// Activate search with a pre-populated value (for editing existing queries)
    pub fn activate_with_value(&mut self, initial_value: String) {
        self.state = SearchState::Active;
        self.keyboard = Some(Keyboard::new(self.res.clone(), initial_value, false));
    }

    /// Cancel search and hide keyboard
    pub fn deactivate(&mut self) {
        self.state = SearchState::Inactive;
        self.keyboard = None;
    }

    /// Check if search is currently active (keyboard visible or searching)
    pub fn is_active(&self) -> bool {
        !matches!(self.state, SearchState::Inactive)
    }

    /// Get current search state
    pub fn state(&self) -> &SearchState {
        &self.state
    }

    /// Check if database needs indexing and emit appropriate commands
    ///
    /// Returns Ok(true) if database is ready, Ok(false) if indexing needed
    async fn check_database(&self, commands: &Sender<Command>) -> Result<bool> {
        if !self.res.get::<Database>().has_indexed()? {
            let toast = self.res.get::<Locale>().t("populating-database");
            commands.send(Command::Toast(toast, None)).await?;
            commands.send(Command::PopulateDb).await?;
            commands
                .send(Command::Toast(String::new(), Some(Duration::ZERO)))
                .await?;
            return Ok(false);
        }
        Ok(true)
    }
}

#[async_trait(?Send)]
impl View for SearchView {
    fn draw(
        &mut self,
        display: &mut <DefaultPlatform as Platform>::Display,
        styles: &Stylesheet,
    ) -> Result<bool> {
        if let Some(keyboard) = self.keyboard.as_mut() {
            if matches!(self.state, SearchState::Active) {
                return keyboard.draw(display, styles);
            }
        }
        Ok(false)
    }

    fn should_draw(&self) -> bool {
        matches!(self.state, SearchState::Active)
            && self
                .keyboard
                .as_ref()
                .map(|k| k.should_draw())
                .unwrap_or(false)
    }

    fn set_should_draw(&mut self) {
        if matches!(self.state, SearchState::Active) {
            if let Some(keyboard) = self.keyboard.as_mut() {
                keyboard.set_should_draw();
            }
        }
    }

    async fn handle_key_event(
        &mut self,
        event: KeyEvent,
        commands: Sender<Command>,
        bubble: &mut VecDeque<Command>,
    ) -> Result<bool> {
        // Only handle events when active
        if !matches!(self.state, SearchState::Active) {
            return Ok(false);
        }

        // Let keyboard handle the event
        if let Some(keyboard) = self.keyboard.as_mut() {
            if keyboard
                .handle_key_event(event, commands.clone(), bubble)
                .await?
            {
                // Process keyboard commands
                let mut query = None;
                bubble.retain_mut(|c| match c {
                    Command::ValueChanged(_, val) => {
                        // User submitted a query
                        if let Value::String(val) = val {
                            query = Some(val.clone());
                        }
                        false // Consume this command
                    }
                    Command::CloseView => {
                        // User cancelled search
                        self.deactivate();
                        false // Consume this command
                    }
                    _ => true, // Let other commands bubble
                });

                // If user submitted a query, check DB and emit search command
                if let Some(query) = query {
                    if self.check_database(&commands).await? {
                        self.state = SearchState::Searching;
                        commands.send(Command::Search(query)).await?;
                    }
                }

                return Ok(true);
            }
        }

        Ok(false)
    }

    fn children(&self) -> Vec<&dyn View> {
        vec![]
    }

    fn children_mut(&mut self) -> Vec<&mut dyn View> {
        vec![]
    }

    fn bounding_box(&mut self, styles: &Stylesheet) -> Rect {
        self.keyboard
            .as_mut()
            .map(|k| k.bounding_box(styles))
            .unwrap_or_else(|| Rect::new(0, 0, 0, 0))
    }

    fn set_position(&mut self, _point: Point) {}
}
