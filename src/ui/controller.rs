//! UI Controller
//!
//! This module contains all UI-specific state management, separated from
//! business logic to enable better testing and clearer separation of concerns.

use crossterm::event::KeyCode;

use crate::app::LoadingState;
use crate::ui_state::{DetailViewState, ListViewState};

/// Manages all UI-related state
pub struct UiController {
    /// UI state for list view
    pub list_view_state: ListViewState,
    /// UI state for detail view
    pub detail_view_state: DetailViewState,
    /// Loading status
    pub loading_state: LoadingState,
    /// Last key pressed (for multi-key sequences)
    pub last_key_press: Option<KeyCode>,
    /// Whether help popup is showing
    pub showing_help: bool,
}

impl UiController {
    /// Create a new UiController with default state
    pub fn new(list_state: ratatui::widgets::ListState) -> Self {
        Self {
            list_view_state: ListViewState::new(list_state),
            detail_view_state: DetailViewState::default(),
            loading_state: LoadingState::Loading,
            last_key_press: None,
            showing_help: false,
        }
    }

    /// Check if currently filtering
    pub fn is_filtering(&self) -> bool {
        self.list_view_state.is_filtering
    }

    /// Check if type picker is open
    pub fn is_type_picker_open(&self) -> bool {
        self.list_view_state.type_picker.is_open
    }

    /// Check if in edit mode
    pub fn is_editing(&self) -> bool {
        self.detail_view_state
            .edit_state
            .as_ref()
            .is_some_and(|s| s.is_editing)
    }

    /// Get current filter query
    pub fn filter_query(&self) -> &str {
        &self.list_view_state.filter_query
    }

    /// Clear filter query
    pub fn clear_filter(&mut self) {
        self.list_view_state.filter_query.clear();
    }

    /// Show/hide help
    pub fn toggle_help(&mut self) {
        self.showing_help = !self.showing_help;
    }

    /// Reset last key press
    pub fn reset_last_key(&mut self) {
        self.last_key_press = None;
    }

    /// Update last key press
    pub fn set_last_key(&mut self, key: KeyCode) {
        self.last_key_press = Some(key);
    }

    /// Check if assigned to me filter is on
    pub fn is_assigned_to_me_filter_on(&self) -> bool {
        self.list_view_state.assigned_to_me_filter_on
    }

    /// Toggle assigned to me filter
    pub fn toggle_assigned_to_me_filter(&mut self) {
        self.list_view_state.assigned_to_me_filter_on =
            !self.list_view_state.assigned_to_me_filter_on;
    }

    /// Open filter input
    pub fn open_filter(&mut self) {
        self.list_view_state.is_filtering = true;
        self.clear_filter();
    }

    /// Close filter input
    pub fn close_filter(&mut self) {
        self.list_view_state.is_filtering = false;
    }

    /// Reset detail edit state
    pub fn reset_edit_state(&mut self) {
        self.detail_view_state.edit_state = None;
        self.detail_view_state.save_status = crate::ui_state::SaveStatus::Idle;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ui_controller_new() {
        let list_state = ratatui::widgets::ListState::default();
        let ui = UiController::new(list_state);

        assert!(!ui.is_filtering());
        assert!(!ui.is_type_picker_open());
        assert!(!ui.is_editing());
        assert!(!ui.showing_help);
        assert_eq!(ui.last_key_press, None);
    }

    #[test]
    fn test_toggle_help() {
        let list_state = ratatui::widgets::ListState::default();
        let mut ui = UiController::new(list_state);

        assert!(!ui.showing_help);
        ui.toggle_help();
        assert!(ui.showing_help);
        ui.toggle_help();
        assert!(!ui.showing_help);
    }

    #[test]
    fn test_filter_operations() {
        let list_state = ratatui::widgets::ListState::default();
        let mut ui = UiController::new(list_state);

        assert!(!ui.is_filtering());
        ui.open_filter();
        assert!(ui.is_filtering());
        ui.list_view_state.filter_query.push_str("test");
        assert_eq!(ui.filter_query(), "test");
        ui.clear_filter();
        assert_eq!(ui.filter_query(), "");
        ui.close_filter();
        assert!(!ui.is_filtering());
    }

    #[test]
    #[test]
    fn test_assigned_to_me_filter() {
        let list_state = ratatui::widgets::ListState::default();
        let mut ui = UiController::new(list_state);

        assert!(!ui.is_assigned_to_me_filter_on());
        ui.toggle_assigned_to_me_filter();
        assert!(ui.is_assigned_to_me_filter_on());
        ui.toggle_assigned_to_me_filter();
        assert!(!ui.is_assigned_to_me_filter_on());
    }

    #[test]
    fn test_key_tracking() {
        let list_state = ratatui::widgets::ListState::default();
        let mut ui = UiController::new(list_state);

        assert_eq!(ui.last_key_press, None);
        ui.set_last_key(crossterm::event::KeyCode::Char('q'));
        assert_eq!(
            ui.last_key_press,
            Some(crossterm::event::KeyCode::Char('q'))
        );
        ui.reset_last_key();
        assert_eq!(ui.last_key_press, None);
    }
}
