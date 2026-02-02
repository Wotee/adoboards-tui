//! Event handlers for different application modes
//!
//! This module breaks down the massive event handler into manageable chunks,
//! with separate functions for each mode (help, filter, picker, edit, normal).

use crossterm::event::KeyCode;

use crate::app::{App, LoadingState, RefreshPolicy};
use crate::input::keymap::key_matches_sequence;
use crate::models::DetailField;
use crate::ui_state::SaveStatus;

/// Result of handling an event
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandlerResult {
    /// Event was handled, continue processing
    Handled,
    /// Event was handled, exit the application
    Exit,
    /// Event was not handled by this handler
    NotHandled,
}

/// Handle events when help popup is showing
pub fn handle_help_mode(app: &mut App, key: KeyCode) -> HandlerResult {
    match key {
        KeyCode::Esc => {
            app.showing_help = false;
            app.last_key_press = None;
            HandlerResult::Handled
        }
        KeyCode::Char(c) => {
            let last_key = app.last_key_press;
            if key_matches_sequence(c, last_key, &app.keys.help)
                || key_matches_sequence(c, last_key, &app.keys.quit)
            {
                app.showing_help = false;
                app.last_key_press = None;
            } else {
                app.last_key_press = Some(key);
            }
            HandlerResult::Handled
        }
        _ => HandlerResult::Handled,
    }
}

/// Handle events when filter input is active
pub fn handle_filter_mode(app: &mut App, key: KeyCode) -> HandlerResult {
    match key {
        KeyCode::Enter | KeyCode::Esc => {
            app.list_view_state.is_filtering = false;
            if key == KeyCode::Esc {
                app.list_view_state.filter_query.clear();
                app.clamp_selection();
            }
            HandlerResult::Handled
        }
        KeyCode::Backspace => {
            app.list_view_state.filter_query.pop();
            app.clamp_selection();
            HandlerResult::Handled
        }
        KeyCode::Char(c) => {
            if c != '/' {
                app.list_view_state.filter_query.push(c);
                app.clamp_selection();
            }
            HandlerResult::Handled
        }
        _ => HandlerResult::Handled,
    }
}

/// Handle events when type picker is open
pub fn handle_type_picker_mode(app: &mut App, key: KeyCode) -> HandlerResult {
    match key {
        KeyCode::Esc => {
            app.list_view_state.type_picker.close();
            HandlerResult::Handled
        }
        KeyCode::Char('c') => {
            app.clear_type_filters();
            app.list_view_state.type_picker.close();
            HandlerResult::Handled
        }
        KeyCode::Enter | KeyCode::Char(' ') => {
            app.toggle_type_selection();
            HandlerResult::Handled
        }
        KeyCode::Up => {
            app.move_type_selection(-1);
            HandlerResult::Handled
        }
        KeyCode::Down => {
            app.move_type_selection(1);
            HandlerResult::Handled
        }
        KeyCode::Char(c) => {
            let last_key = app.last_key_press;
            if key_matches_sequence(c, last_key, &app.keys.quit) {
                app.list_view_state.type_picker.close();
                app.last_key_press = None;
            } else if key_matches_sequence(c, last_key, &app.keys.next) {
                app.move_type_selection(1);
                app.last_key_press = Some(key);
            } else if key_matches_sequence(c, last_key, &app.keys.previous) {
                app.move_type_selection(-1);
                app.last_key_press = Some(key);
            } else {
                app.last_key_press = None;
            }
            HandlerResult::Handled
        }
        _ => HandlerResult::Handled,
    }
}

/// Check if currently in edit mode with an active picker
fn has_active_picker(app: &App) -> bool {
    app.detail_view_state
        .edit_state
        .as_ref()
        .and_then(|s| {
            match s.active_field {
                DetailField::Dynamic(idx) => s.visible_fields.get(idx),
                DetailField::Title => None,
            }
        })
        .and_then(|f| f.picker.as_ref())
        .is_some()
}

/// Handle edit mode key events
fn handle_edit_keys(app: &mut App, key: char, last_key: Option<KeyCode>) -> HandlerResult {
    if key_matches_sequence(key, last_key, &app.keys.next) {
        if has_active_picker(app) {
            app.move_active_picker(1);
        }
        app.last_key_press = Some(KeyCode::Char(key));
        HandlerResult::Handled
    } else if key_matches_sequence(key, last_key, &app.keys.previous) {
        if has_active_picker(app) {
            app.move_active_picker(-1);
        }
        app.last_key_press = Some(KeyCode::Char(key));
        HandlerResult::Handled
    } else {
        HandlerResult::NotHandled
    }
}

/// Handle normal mode key events (non-edit mode)
fn handle_normal_keys(app: &mut App, key: char, last_key: Option<KeyCode>) -> HandlerResult {
    if key_matches_sequence(key, last_key, &app.keys.quit) {
        return HandlerResult::Exit;
    }

    if key_matches_sequence(key, last_key, &app.keys.help) {
        app.showing_help = !app.showing_help;
        app.last_key_press = None;
        return HandlerResult::Handled;
    }

    if key_matches_sequence(key, last_key, &app.keys.jump_to_top) {
        app.jump_to_start();
    } else if key_matches_sequence(key, last_key, &app.keys.jump_to_end) {
        app.jump_to_end();
    } else if key_matches_sequence(key, last_key, &app.keys.search) {
        app.list_view_state.is_filtering = true;
        app.list_view_state.filter_query.clear();
        app.clamp_selection();
    } else if key_matches_sequence(key, last_key, &app.keys.next) {
        app.navigate_list(1);
    } else if key_matches_sequence(key, last_key, &app.keys.previous) {
        app.navigate_list(-1);
    } else if key_matches_sequence(key, last_key, &app.keys.next_board) {
        app.next_source();
        return HandlerResult::Exit;
    } else if key_matches_sequence(key, last_key, &app.keys.previous_board) {
        app.previous_source();
        return HandlerResult::Exit;
    } else if key_matches_sequence(key, last_key, &app.keys.open) {
        app.open_item();
    } else if key_matches_sequence(key, last_key, &app.keys.assigned_to_me_filter) {
        app.toggle_assigned_to_me_filter();
    } else if key_matches_sequence(key, last_key, &app.keys.work_item_type_filter) {
        app.toggle_type_filter_menu();
    } else if key_matches_sequence(key, last_key, &app.keys.refresh) {
        app.refresh_policy = RefreshPolicy::Normal;
        app.loading_state = LoadingState::Loading;
        return HandlerResult::Exit;
    } else if key_matches_sequence(key, last_key, &app.keys.full_refresh) {
        app.refresh_policy = RefreshPolicy::Full;
        app.loading_state = LoadingState::Loading;
        return HandlerResult::Exit;
    } else if key_matches_sequence(key, last_key, &app.keys.edit_config) {
        let _ = crate::config::open_config();
        eprintln!("Reopen adoboards for changes to take effect");
        return HandlerResult::Exit;
    } else if key_matches_sequence(key, last_key, &app.keys.edit_item) {
        // Note: This is async in the original, handled separately
        return HandlerResult::NotHandled;
    } else {
        return HandlerResult::NotHandled;
    }

    app.last_key_press = Some(KeyCode::Char(key));
    HandlerResult::Handled
}

/// Handle special keys (non-character keys)
fn handle_special_keys(app: &mut App, key: KeyCode, editing_active: bool) -> HandlerResult {
    match key {
        KeyCode::Esc => {
            if editing_active {
                app.cancel_edit();
            } else {
                if app.list_view_state.assigned_to_me_filter_on {
                    app.toggle_assigned_to_me_filter();
                }
                if !app.list_view_state.filter_query.is_empty() {
                    app.list_view_state.filter_query.clear();
                    app.clamp_selection();
                }
                if app.list_view_state.type_picker.is_open {
                    app.toggle_type_filter_menu();
                }
                app.detail_view_state.edit_state = None;
            }
            HandlerResult::Handled
        }
        KeyCode::Up => {
            if editing_active {
                app.move_active_picker(-1);
            } else {
                app.navigate_list(-1);
            }
            HandlerResult::Handled
        }
        KeyCode::Down => {
            if editing_active {
                app.move_active_picker(1);
            } else {
                app.navigate_list(1);
            }
            HandlerResult::Handled
        }
        KeyCode::Left => {
            if editing_active {
                app.detail_view_state.edit_state.as_mut().map(|state| {
                    if let DetailField::Dynamic(idx) = state.active_field {
                        let new_idx = idx.saturating_sub(1);
                        state.active_field = DetailField::Dynamic(new_idx);
                    }
                    state
                });
            } else {
            }
            HandlerResult::Handled
        }
        KeyCode::Right => {
            if editing_active {
                app.detail_view_state.edit_state.as_mut().map(|state| {
                    if let DetailField::Dynamic(idx) = state.active_field {
                        let new_idx = idx + 1;
                        if new_idx < state.visible_fields.len() {
                            state.active_field = DetailField::Dynamic(new_idx);
                        }
                    }
                    state
                });
            } else {
            }
            HandlerResult::Handled
        }
        KeyCode::Enter => {
            if editing_active {
                // TODO: Handle async begin_save in integration step
                // app.begin_save().await;
            }
            HandlerResult::Handled
        }
        _ => HandlerResult::NotHandled,
    }
}

/// Handle events in normal/edit mode
pub async fn handle_normal_mode(app: &mut App, key: KeyCode) -> HandlerResult {
    // Check for ongoing save operations
    app.poll_save_completion();

    if matches!(app.detail_view_state.save_status, SaveStatus::Saving) {
        app.last_key_press = None;
        return HandlerResult::Handled;
    }

    let editing_active = app
        .detail_view_state
        .edit_state
        .as_ref()
        .is_some_and(|s| s.is_editing);

    // Handle special keys first
    let result = handle_special_keys(app, key, editing_active);
    if result != HandlerResult::NotHandled {
        return result;
    }

    // Handle character keys
    if let KeyCode::Char(c) = key {
        let last_key = app.last_key_press;

        // Try edit mode handlers first if editing
        if editing_active {
            let result = handle_edit_keys(app, c, last_key);
            if result != HandlerResult::NotHandled {
                return result;
            }

            // Apply typing if not handled by edit keys
            app.apply_typing(c);
            app.last_key_press = None;
            return HandlerResult::Handled;
        }

        // Try normal mode handlers
        let result = handle_normal_keys(app, c, last_key);
        if result != HandlerResult::NotHandled {
            return result;
        }

        // Handle edit_item specially (it's async)
        if key_matches_sequence(c, last_key, &app.keys.edit_item) {
            app.ensure_detail_state_for_selected_item().await;
            app.begin_edit();
            app.last_key_press = Some(key);
            return HandlerResult::Handled;
        }

        app.last_key_press = Some(key);
        return HandlerResult::Handled;
    }

    HandlerResult::Handled
}

/// Main event dispatcher - routes events to appropriate handler based on mode
pub async fn dispatch_event(app: &mut App, key: KeyCode) -> HandlerResult {
    if app.showing_help {
        return handle_help_mode(app, key);
    }

    if app.list_view_state.is_filtering {
        return handle_filter_mode(app, key);
    }

    if app.list_view_state.type_picker.is_open {
        return handle_type_picker_mode(app, key);
    }

    handle_normal_mode(app, key).await
}

#[cfg(test)]
mod tests {
    use super::*;

    // Note: These tests would need a mock App to work properly
    // For now, we just test the handler result types

    #[test]
    fn test_handler_result_variants() {
        assert!(matches!(HandlerResult::Handled, HandlerResult::Handled));
        assert!(matches!(HandlerResult::Exit, HandlerResult::Exit));
        assert!(matches!(HandlerResult::NotHandled, HandlerResult::NotHandled));
    }

    #[test]
    fn test_handler_result_equality() {
        assert_eq!(HandlerResult::Handled, HandlerResult::Handled);
        assert_ne!(HandlerResult::Handled, HandlerResult::Exit);
        assert_ne!(HandlerResult::Handled, HandlerResult::NotHandled);
        assert_ne!(HandlerResult::Exit, HandlerResult::NotHandled);
    }
}
