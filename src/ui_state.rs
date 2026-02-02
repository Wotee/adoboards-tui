//! UI State Management
//!
//! This module contains all UI-related state structures that control the visual
//! presentation and user interaction state of the application.

use ratatui::widgets::ListState;
use tokio::sync::oneshot;

use crate::config::IterationConfig;
use crate::models::{DetailField, WorkItem};
use crate::picker::PickerState;

/// State for the list view component
#[derive(Clone)]
pub struct ListViewState {
    pub list_state: ListState,
    pub filter_query: String,
    pub is_filtering: bool,
    pub assigned_to_me_filter_on: bool,
    pub type_picker: PickerState,
}

impl ListViewState {
    pub fn new(list_state: ListState) -> Self {
        Self {
            list_state,
            filter_query: String::new(),
            is_filtering: false,
            assigned_to_me_filter_on: false,
            type_picker: PickerState::default(),
        }
    }
}

impl Default for ListViewState {
    fn default() -> Self {
        Self::new(ListState::default())
    }
}

/// A visible field in the detail view with optional picker for editing
#[derive(Clone)]
pub struct VisibleField {
    pub label: String,
    pub reference: String,
    pub value: String,
    pub picker: Option<PickerState>,
}

impl VisibleField {
    pub fn with_value(
        label: String,
        reference: String,
        value: String,
        allowed_values: Option<Vec<String>>,
    ) -> Self {
        let mut picker = allowed_values.and_then(|values| {
            if values.is_empty() {
                None
            } else {
                Some(PickerState::from_options(values))
            }
        });
        if let Some(ref mut p) = picker {
            p.set_selected_to_value(&value);
        }
        Self {
            label,
            reference,
            value,
            picker,
        }
    }

    pub fn select_value(&mut self, idx: usize) {
        if let Some(picker) = self.picker.as_mut() {
            if let Some(choice) = picker.options.get(idx).cloned() {
                self.value = choice;
                picker.selected = Some(idx);
            }
        }
    }
}

/// State for editing a work item in the detail view
#[derive(Clone)]
pub struct DetailEditState {
    pub is_editing: bool,
    pub active_field: DetailField,
    pub title: String,
    pub visible_fields: Vec<VisibleField>,
}

impl DetailEditState {
    pub fn new_from_item(item: &WorkItem) -> Self {
        Self {
            is_editing: false,
            active_field: DetailField::Title,
            title: item.title.clone(),
            visible_fields: Vec::new(),
        }
    }
}

/// State for the detail view component
#[derive(Default)]
pub struct DetailViewState {
    pub edit_state: Option<DetailEditState>,
    pub save_status: SaveStatus,
    pub save_receiver: Option<oneshot::Receiver<anyhow::Result<(WorkItem, DetailEditState)>>>,
}

/// Status of save operations
#[derive(Default, Clone)]
pub enum SaveStatus {
    #[default]
    Idle,
    Saving,
    Failed(String),
}

/// Type of data source (backlog or iteration)
#[derive(Clone)]
pub enum SourceKind {
    Backlog,
    Iteration(IterationConfig),
}

/// A source entry representing a board or iteration
#[derive(Clone)]
pub struct SourceEntry {
    pub title: String,
    pub team: String,
    pub organization: String,
    pub project: String,
    pub kind: SourceKind,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    #[test]
    fn test_list_view_state_default() {
        let state = ListViewState::default();
        assert!(state.filter_query.is_empty());
        assert!(!state.is_filtering);
        assert!(!state.assigned_to_me_filter_on);
    }

    #[test]
    fn test_list_view_state_new() {
        let mut list_state = ListState::default();
        list_state.select(Some(5));
        let state = ListViewState::new(list_state);
        assert_eq!(state.list_state.selected(), Some(5));
    }

    #[test]
    fn test_visible_field_with_value() {
        let field = VisibleField::with_value(
            "Priority".to_string(),
            "System.Priority".to_string(),
            "High".to_string(),
            Some(vec![
                "Low".to_string(),
                "Medium".to_string(),
                "High".to_string(),
            ]),
        );
        assert_eq!(field.label, "Priority");
        assert_eq!(field.value, "High");
        assert!(field.picker.is_some());
        let picker = field.picker.as_ref().unwrap();
        // Options are sorted alphabetically: High, Low, Medium
        assert_eq!(picker.selected, Some(0)); // "High" is at index 0
        assert!(picker.active.is_empty());
    }

    #[test]
    fn test_visible_field_no_picker_for_empty_allowed_values() {
        let field = VisibleField::with_value(
            "Title".to_string(),
            "System.Title".to_string(),
            "Test Title".to_string(),
            Some(vec![]),
        );
        assert!(field.picker.is_none());
    }

    #[test]
    fn test_visible_field_select_value() {
        let mut field = VisibleField::with_value(
            "State".to_string(),
            "System.State".to_string(),
            "New".to_string(),
            Some(vec![
                "New".to_string(),
                "Active".to_string(),
                "Resolved".to_string(),
            ]),
        );
        // Options are sorted alphabetically: Active, New, Resolved
        // "New" is at index 1, so let's select "Active" at index 0
        field.select_value(0);
        assert_eq!(field.value, "Active");
        assert_eq!(field.picker.as_ref().unwrap().selected, Some(0));
    }

    #[test]
    fn test_visible_field_select_value_out_of_bounds() {
        let mut field = VisibleField::with_value(
            "State".to_string(),
            "System.State".to_string(),
            "New".to_string(),
            Some(vec!["New".to_string(), "Active".to_string()]),
        );
        // Options are sorted alphabetically: Active, New
        // "New" is at index 1
        field.select_value(10); // Out of bounds
        assert_eq!(field.value, "New"); // Value unchanged
        assert_eq!(field.picker.as_ref().unwrap().selected, Some(1)); // Still at original
    }

    #[test]
    fn test_detail_edit_state_new_from_item() {
        let item = WorkItem {
            id: 123,
            title: "Test Work Item".to_string(),
            assigned_to: "Alice".to_string(),
            state: "Active".to_string(),
            work_item_type: "Bug".to_string(),
            description: "Description".to_string(),
            acceptance_criteria: "AC".to_string(),
            fields: BTreeMap::new(),
        };
        let edit_state = DetailEditState::new_from_item(&item);
        assert!(!edit_state.is_editing);
        assert_eq!(edit_state.active_field, DetailField::Title);
        assert_eq!(edit_state.title, "Test Work Item");
        assert!(edit_state.visible_fields.is_empty());
    }

    #[test]
    fn test_detail_view_state_default() {
        let state = DetailViewState::default();
        assert!(state.edit_state.is_none());
        assert!(matches!(state.save_status, SaveStatus::Idle));
        assert!(state.save_receiver.is_none());
    }

    #[test]
    fn test_save_status_variants() {
        let idle = SaveStatus::Idle;
        let saving = SaveStatus::Saving;
        let failed = SaveStatus::Failed("Network error".to_string());

        assert!(matches!(idle, SaveStatus::Idle));
        assert!(matches!(saving, SaveStatus::Saving));
        assert!(matches!(failed, SaveStatus::Failed(_)));
    }

    #[test]
    fn test_source_kind_variants() {
        let backlog = SourceKind::Backlog;
        let iteration = SourceKind::Iteration(IterationConfig {
            organization: "org".to_string(),
            project: "proj".to_string(),
            team: "team".to_string(),
            iteration: "Sprint 1".to_string(),
        });

        assert!(matches!(backlog, SourceKind::Backlog));
        assert!(matches!(iteration, SourceKind::Iteration(_)));
    }

    #[test]
    fn test_source_entry_creation() {
        let entry = SourceEntry {
            title: "My Team Backlog".to_string(),
            team: "My Team".to_string(),
            organization: "myorg".to_string(),
            project: "myproject".to_string(),
            kind: SourceKind::Backlog,
        };

        assert_eq!(entry.title, "My Team Backlog");
        assert_eq!(entry.team, "My Team");
        assert_eq!(entry.organization, "myorg");
        assert_eq!(entry.project, "myproject");
    }
}
