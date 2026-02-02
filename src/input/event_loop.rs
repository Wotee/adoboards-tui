//! Main event loop for the application
//!
//! This module encapsulates the terminal event loop, separating the event handling
//! logic from the application state management.

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode};
use ratatui::{Terminal, backend::Backend};

use crate::app::{App, LoadingState, RefreshPolicy};
use crate::input::keymap::key_matches_sequence;
use crate::ui::{draw_detail_view, draw_list_view, draw_help_popup, draw_status_screen};

/// Encapsulates the main terminal event loop
pub struct EventLoop<'a, B: Backend> {
    terminal: &'a mut Terminal<B>,
    app: &'a mut App,
}

impl<'a, B: Backend> EventLoop<'a, B> {
    /// Create a new EventLoop with the given terminal and app
    pub fn new(terminal: &'a mut Terminal<B>, app: &'a mut App) -> Self {
        Self { terminal, app }
    }

    /// Run the main event loop until the application exits
    pub async fn run(&mut self) -> io::Result<()> {
        if matches!(self.app.loading_state, LoadingState::Loading) {
            return Ok(());
        }

        loop {
            self.draw()?;

            if event::poll(Duration::from_millis(100))? {
                if let Event::Key(key) = event::read()? {
                    if let Some(result) = self.handle_key_event(key.code).await {
                        return result;
                    }
                }
            }
        }
    }

    /// Draw the current application state to the terminal
    fn draw(&mut self) -> io::Result<()> {
        self.terminal.draw(|f| match self.app.loading_state {
            LoadingState::Loaded => {
                let main_chunks = ratatui::layout::Layout::default()
                    .direction(ratatui::layout::Direction::Horizontal)
                    .constraints([
                        ratatui::layout::Constraint::Percentage(38),
                        ratatui::layout::Constraint::Percentage(62),
                    ])
                    .split(f.area());

                draw_list_view(f, self.app, main_chunks[0]);
                draw_detail_view(f, self.app, main_chunks[1]);
                draw_help_popup(f, self.app);
            }
            LoadingState::Loading => {}
            LoadingState::Error(ref msg) => {
                draw_status_screen(f, &format!("Failed to load data. {}", msg))
            }
        })?;
        Ok(())
    }

    /// Handle a single key event
    /// Returns Some(Ok(())) to exit successfully, Some(Err) for errors, None to continue
    async fn handle_key_event(&mut self, key: KeyCode) -> Option<io::Result<()>> {
        // Handle loading/error states first
        match self.app.loading_state {
            LoadingState::Loading | LoadingState::Error(_) => {
                match key {
                    KeyCode::Char('q') | KeyCode::Esc => return Some(Ok(())),
                    _ => return None,
                }
            }
            _ => {}
        }

        // Delegate to mode-specific handlers based on current state
        if self.app.showing_help {
            self.handle_help_mode(key).await
        } else if self.app.list_view_state.is_filtering {
            self.handle_filter_mode(key).await
        } else if self.app.list_view_state.type_picker.is_open {
            self.handle_type_picker_mode(key).await
        } else {
            self.handle_normal_mode(key).await
        }
    }

    /// Handle events when help popup is showing
    async fn handle_help_mode(&mut self, key: KeyCode) -> Option<io::Result<()>> {
        match key {
            KeyCode::Esc => {
                self.app.showing_help = false;
                self.app.last_key_press = None;
            }
            KeyCode::Char(c) => {
                let last_key = self.app.last_key_press;
                if key_matches_sequence(c, last_key, &self.app.keys.help)
                    || key_matches_sequence(c, last_key, &self.app.keys.quit)
                {
                    self.app.showing_help = false;
                    self.app.last_key_press = None;
                } else {
                    self.app.last_key_press = Some(key);
                }
            }
            _ => {}
        }
        None
    }

    /// Handle events when filter input is active
    async fn handle_filter_mode(&mut self, key: KeyCode) -> Option<io::Result<()>> {
        match key {
            KeyCode::Enter | KeyCode::Esc => {
                self.app.list_view_state.is_filtering = false;
                if key == KeyCode::Esc {
                    self.app.list_view_state.filter_query.clear();
                    self.app.clamp_selection();
                }
            }
            KeyCode::Backspace => {
                self.app.list_view_state.filter_query.pop();
                self.app.clamp_selection();
            }
            KeyCode::Char(c) => {
                if c != '/' {
                    self.app.list_view_state.filter_query.push(c);
                    self.app.clamp_selection();
                }
            }
            _ => {}
        }
        None
    }

    /// Handle events when type picker is open
    async fn handle_type_picker_mode(&mut self, key: KeyCode) -> Option<io::Result<()>> {
        match key {
            KeyCode::Esc => {
                self.app.list_view_state.type_picker.close();
            }
            KeyCode::Char('c') => {
                self.app.clear_type_filters();
                self.app.list_view_state.type_picker.close();
            }
            KeyCode::Enter | KeyCode::Char(' ') => {
                self.app.toggle_type_selection();
            }
            KeyCode::Up => {
                self.app.move_type_selection(-1);
            }
            KeyCode::Down => {
                self.app.move_type_selection(1);
            }
            KeyCode::Char(c) => {
                let last_key = self.app.last_key_press;
                if key_matches_sequence(c, last_key, &self.app.keys.quit) {
                    self.app.list_view_state.type_picker.close();
                    self.app.last_key_press = None;
                } else if key_matches_sequence(c, last_key, &self.app.keys.next) {
                    self.app.move_type_selection(1);
                    self.app.last_key_press = Some(key);
                } else if key_matches_sequence(c, last_key, &self.app.keys.previous) {
                    self.app.move_type_selection(-1);
                    self.app.last_key_press = Some(key);
                } else {
                    self.app.last_key_press = None;
                }
            }
            _ => {}
        }
        None
    }

    /// Handle events in normal/edit mode
    async fn handle_normal_mode(&mut self, key: KeyCode) -> Option<io::Result<()>> {
        use crate::ui_state::SaveStatus;
        use crate::models::DetailField;

        // Check for ongoing save operations
        self.app.poll_save_completion();

        if matches!(self.app.detail_view_state.save_status, SaveStatus::Saving) {
            self.app.last_key_press = None;
            return None;
        }

        let editing_active = self.app
            .detail_view_state
            .edit_state
            .as_ref()
            .is_some_and(|s| s.is_editing);

        match key {
            KeyCode::Esc => {
                if editing_active {
                    self.app.cancel_edit();
                } else {
                    if self.app.list_view_state.assigned_to_me_filter_on {
                        self.app.toggle_assigned_to_me_filter();
                    }
                    if !self.app.list_view_state.filter_query.is_empty() {
                        self.app.list_view_state.filter_query.clear();
                        self.app.clamp_selection();
                    }
                    if self.app.list_view_state.type_picker.is_open {
                        self.app.toggle_type_filter_menu();
                    }
                    self.app.detail_view_state.edit_state = None;
                }
            }
            KeyCode::Up => {
                if editing_active {
                    self.app.move_active_picker(-1);
                } else {
                    self.app.navigate_list(-1);
                }
            }
            KeyCode::Down => {
                if editing_active {
                    self.app.move_active_picker(1);
                } else {
                    self.app.navigate_list(1);
                }
            }
            KeyCode::Tab => {
                if editing_active {
                    self.app.detail_view_state.edit_state.as_mut().map(|state| {
                        match state.active_field {
                            DetailField::Title => {
                                if !state.visible_fields.is_empty() {
                                    state.active_field = DetailField::Dynamic(0);
                                }
                            }
                            DetailField::Dynamic(idx) => {
                                let new_idx = idx + 1;
                                if new_idx < state.visible_fields.len() {
                                    state.active_field = DetailField::Dynamic(new_idx);
                                }
                            }
                        }
                        state
                    });
                }
            }
            KeyCode::BackTab => {
                if editing_active {
                    self.app.detail_view_state.edit_state.as_mut().map(|state| {
                        match state.active_field {
                            DetailField::Title => {}
                            DetailField::Dynamic(idx) => {
                                let new_idx = idx.saturating_sub(1);
                                if new_idx == 0 && idx == 0 {
                                    state.active_field = DetailField::Title;
                                } else {
                                    state.active_field = DetailField::Dynamic(new_idx);
                                }
                            }
                        }
                        state
                    });
                }
            }
            KeyCode::Enter => {
                if editing_active {
                    self.app.start_save();
                }
            }
            KeyCode::Backspace => {
                if editing_active {
                    self.app.apply_backspace();
                }
            }
            KeyCode::Char(c) => {
                return self.handle_char_key(c, editing_active).await;
            }
            _ => {}
        }

        None
    }

    /// Handle character key presses in normal mode
    async fn handle_char_key(&mut self, c: char, editing_active: bool) -> Option<io::Result<()>> {
        let last_key = self.app.last_key_press;

        // Handle edit mode keys first (before quit/help)
        if editing_active {
            // Check if there's an active picker
            let has_picker = self.app
                .detail_view_state
                .edit_state
                .as_ref()
                .and_then(|s| {
                    match s.active_field {
                        crate::models::DetailField::Dynamic(idx) => s.visible_fields.get(idx),
                        crate::models::DetailField::Title => None,
                    }
                })
                .and_then(|f| f.picker.as_ref())
                .is_some();

            if key_matches_sequence(c, last_key, &self.app.keys.next) {
                if has_picker {
                    self.app.move_active_picker(1);
                }
                self.app.last_key_press = Some(KeyCode::Char(c));
                return None;
            } else if key_matches_sequence(c, last_key, &self.app.keys.previous) {
                if has_picker {
                    self.app.move_active_picker(-1);
                }
                self.app.last_key_press = Some(KeyCode::Char(c));
                return None;
            }

            // Apply typing
            self.app.apply_typing(c);
            self.app.last_key_press = None;
            return None;
        }

        // Handle quit (only in normal mode)
        if key_matches_sequence(c, last_key, &self.app.keys.quit) {
            return Some(Ok(()));
        }

        // Handle help (only in normal mode)
        if key_matches_sequence(c, last_key, &self.app.keys.help) {
            self.app.showing_help = !self.app.showing_help;
            self.app.last_key_press = None;
            return None;
        }

        // Handle normal mode navigation
        if key_matches_sequence(c, last_key, &self.app.keys.jump_to_top) {
            self.app.jump_to_start();
        } else if key_matches_sequence(c, last_key, &self.app.keys.jump_to_end) {
            self.app.jump_to_end();
        } else if key_matches_sequence(c, last_key, &self.app.keys.search) {
            self.app.list_view_state.is_filtering = true;
            self.app.list_view_state.filter_query.clear();
            self.app.clamp_selection();
        } else if key_matches_sequence(c, last_key, &self.app.keys.next) {
            self.app.navigate_list(1);
        } else if key_matches_sequence(c, last_key, &self.app.keys.previous) {
            self.app.navigate_list(-1);
        } else if key_matches_sequence(c, last_key, &self.app.keys.next_board) {
            self.app.next_source();
            return Some(Ok(()));
        } else if key_matches_sequence(c, last_key, &self.app.keys.previous_board) {
            self.app.previous_source();
            return Some(Ok(()));
        } else if key_matches_sequence(c, last_key, &self.app.keys.open) {
            self.app.open_item();
        } else if key_matches_sequence(c, last_key, &self.app.keys.assigned_to_me_filter) {
            self.app.toggle_assigned_to_me_filter();
        } else if key_matches_sequence(c, last_key, &self.app.keys.work_item_type_filter) {
            self.app.toggle_type_filter_menu();
        } else if key_matches_sequence(c, last_key, &self.app.keys.refresh) {
            self.app.refresh_policy = RefreshPolicy::Normal;
            self.app.loading_state = LoadingState::Loading;
            return Some(Ok(()));
        } else if key_matches_sequence(c, last_key, &self.app.keys.full_refresh) {
            self.app.refresh_policy = RefreshPolicy::Full;
            self.app.loading_state = LoadingState::Loading;
            return Some(Ok(()));
        } else if key_matches_sequence(c, last_key, &self.app.keys.edit_config) {
            let _ = crate::config::open_config();
            eprintln!("Reopen adoboards for changes to take effect");
            return Some(Ok(()));
        } else if key_matches_sequence(c, last_key, &self.app.keys.edit_item) {
            self.app.ensure_detail_state_for_selected_item().await;
            self.app.begin_edit();
        } else {
            // Unhandled character key
            self.app.last_key_press = Some(KeyCode::Char(c));
            return None;
        }

        self.app.last_key_press = Some(KeyCode::Char(c));
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_loop_creation() {
        // This is a basic structural test
        // Full testing would require mock terminal and app
        assert!(true);
    }
}
