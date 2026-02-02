//! Board State Management
//!
//! This module contains the core business logic state for managing work items,
//! data sources, and caches. It is separate from UI state to enable better
//! testing and maintainability.

use std::collections::{BTreeMap, HashMap};

use crate::models::WorkItem;
use crate::services::WorkItemFieldInfo;
use crate::ui_state::{SourceEntry, SourceKind};

/// Core business state for the application
pub struct BoardState {
    /// Work items for the current source
    pub items: Vec<WorkItem>,
    /// Available data sources (boards and iterations)
    pub sources: Vec<SourceEntry>,
    /// Currently selected source index
    pub current_source_index: usize,
    /// Work item type mappings (display_name -> reference_name)
    pub work_item_types: BTreeMap<String, String>,
    /// Process template type for the current project
    pub process_template_type: Option<String>,
    /// Cache of layout controls (org, project, type) -> [(id, label)]
    pub layout_cache: HashMap<(String, String, String), Vec<(String, String)>>,
    /// Cache of field metadata (reference_name) -> [field_info]
    pub field_meta_cache: HashMap<String, Vec<WorkItemFieldInfo>>,
}

impl BoardState {
    /// Create a new BoardState from configuration
    pub fn new(
        _me: &str,
        boards: &[crate::config::BoardConfig],
        iterations: &[crate::config::IterationConfig],
    ) -> Self {
        let mut sources: Vec<SourceEntry> = Vec::new();

        for board in boards {
            sources.push(SourceEntry {
                title: format!("{} Backlog", board.team),
                team: board.team.clone(),
                organization: board.organization.clone(),
                project: board.project.clone(),
                kind: SourceKind::Backlog,
            });
        }

        for iteration in iterations {
            sources.push(SourceEntry {
                title: format!("{} Iteration: {}", iteration.team, iteration.iteration),
                team: iteration.team.clone(),
                organization: iteration.organization.clone(),
                project: iteration.project.clone(),
                kind: SourceKind::Iteration(iteration.clone()),
            });
        }

        Self {
            items: Vec::new(),
            sources,
            current_source_index: 0,
            work_item_types: BTreeMap::new(),
            process_template_type: None,
            layout_cache: HashMap::new(),
            field_meta_cache: HashMap::new(),
        }
    }

    /// Get the currently selected source
    pub fn current_source(&self) -> Option<&SourceEntry> {
        self.sources.get(self.current_source_index)
    }

    /// Set work item types and clear related caches
    pub fn set_work_item_types(&mut self, types: BTreeMap<String, String>) {
        self.work_item_types = types;
        self.clear_layout_cache();
        self.field_meta_cache.clear();
    }

    /// Clear the layout cache
    pub fn clear_layout_cache(&mut self) {
        self.layout_cache.clear();
    }

    /// Set the process template type and clear related caches
    pub fn set_process_template_type(&mut self, process_template_type: String) {
        self.process_template_type = Some(process_template_type);
        self.clear_layout_cache();
        self.field_meta_cache.clear();
    }

    /// Navigate to the next source
    pub fn next_source(&mut self) {
        if !self.sources.is_empty() {
            self.current_source_index = (self.current_source_index + 1) % self.sources.len();
        }
    }

    /// Navigate to the previous source
    pub fn previous_source(&mut self) {
        if !self.sources.is_empty() {
            self.current_source_index = if self.current_source_index == 0 {
                self.sources.len() - 1
            } else {
                self.current_source_index - 1
            };
        }
    }

    /// Get the number of sources
    pub fn source_count(&self) -> usize {
        self.sources.len()
    }

    /// Check if there are any sources
    pub fn has_sources(&self) -> bool {
        !self.sources.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_board_config() -> crate::config::BoardConfig {
        crate::config::BoardConfig {
            organization: "test-org".to_string(),
            project: "test-project".to_string(),
            team: "test-team".to_string(),
        }
    }

    fn create_test_iteration_config() -> crate::config::IterationConfig {
        crate::config::IterationConfig {
            organization: "test-org".to_string(),
            project: "test-project".to_string(),
            team: "test-team".to_string(),
            iteration: "Sprint 1".to_string(),
        }
    }

    #[test]
    fn test_board_state_new_empty() {
        let state = BoardState::new("me", &[], &[]);
        assert!(state.items.is_empty());
        assert!(state.sources.is_empty());
        assert_eq!(state.current_source_index, 0);
        assert!(state.work_item_types.is_empty());
        assert!(state.process_template_type.is_none());
    }

    #[test]
    fn test_board_state_new_with_boards() {
        let boards = vec![create_test_board_config()];
        let state = BoardState::new("me", &boards, &[]);
        assert_eq!(state.sources.len(), 1);
        assert_eq!(state.sources[0].title, "test-team Backlog");
        assert!(matches!(state.sources[0].kind, SourceKind::Backlog));
    }

    #[test]
    fn test_board_state_new_with_iterations() {
        let iterations = vec![create_test_iteration_config()];
        let state = BoardState::new("me", &[], &iterations);
        assert_eq!(state.sources.len(), 1);
        assert_eq!(state.sources[0].title, "test-team Iteration: Sprint 1");
        assert!(matches!(state.sources[0].kind, SourceKind::Iteration(_)));
    }

    #[test]
    fn test_board_state_new_with_both() {
        let boards = vec![create_test_board_config()];
        let iterations = vec![create_test_iteration_config()];
        let state = BoardState::new("me", &boards, &iterations);
        assert_eq!(state.sources.len(), 2);
        assert!(matches!(state.sources[0].kind, SourceKind::Backlog));
        assert!(matches!(state.sources[1].kind, SourceKind::Iteration(_)));
    }

    #[test]
    fn test_current_source_with_data() {
        let boards = vec![create_test_board_config()];
        let state = BoardState::new("me", &boards, &[]);
        let source = state.current_source();
        assert!(source.is_some());
        assert_eq!(source.unwrap().team, "test-team");
    }

    #[test]
    fn test_current_source_empty() {
        let state = BoardState::new("me", &[], &[]);
        let source = state.current_source();
        assert!(source.is_none());
    }

    #[test]
    fn test_set_work_item_types_clears_caches() {
        let mut state = BoardState::new("me", &[], &[]);

        // Populate caches
        state
            .layout_cache
            .insert(("a".to_string(), "b".to_string(), "c".to_string()), vec![]);
        state.field_meta_cache.insert("field".to_string(), vec![]);

        // Set work item types
        let mut types = BTreeMap::new();
        types.insert("Bug".to_string(), "System.Bug".to_string());
        state.set_work_item_types(types);

        // Caches should be cleared
        assert!(state.layout_cache.is_empty());
        assert!(state.field_meta_cache.is_empty());
        assert_eq!(state.work_item_types.len(), 1);
    }

    #[test]
    fn test_set_process_template_type_clears_caches() {
        let mut state = BoardState::new("me", &[], &[]);

        // Populate caches
        state
            .layout_cache
            .insert(("a".to_string(), "b".to_string(), "c".to_string()), vec![]);
        state.field_meta_cache.insert("field".to_string(), vec![]);

        // Set process template type
        state.set_process_template_type("Agile".to_string());

        // Caches should be cleared
        assert!(state.layout_cache.is_empty());
        assert!(state.field_meta_cache.is_empty());
        assert_eq!(state.process_template_type, Some("Agile".to_string()));
    }

    #[test]
    fn test_next_source_wraps_around() {
        let boards = vec![
            create_test_board_config(),
            crate::config::BoardConfig {
                team: "team-2".to_string(),
                ..create_test_board_config()
            },
        ];
        let mut state = BoardState::new("me", &boards, &[]);

        assert_eq!(state.current_source_index, 0);
        state.next_source();
        assert_eq!(state.current_source_index, 1);
        state.next_source();
        assert_eq!(state.current_source_index, 0); // Wraps around
    }

    #[test]
    fn test_previous_source_wraps_around() {
        let boards = vec![
            create_test_board_config(),
            crate::config::BoardConfig {
                team: "team-2".to_string(),
                ..create_test_board_config()
            },
        ];
        let mut state = BoardState::new("me", &boards, &[]);

        assert_eq!(state.current_source_index, 0);
        state.previous_source();
        assert_eq!(state.current_source_index, 1); // Wraps to last
        state.previous_source();
        assert_eq!(state.current_source_index, 0);
    }

    #[test]
    fn test_navigation_with_empty_sources() {
        let mut state = BoardState::new("me", &[], &[]);
        state.next_source(); // Should not panic
        state.previous_source(); // Should not panic
        assert_eq!(state.current_source_index, 0);
    }

    #[test]
    fn test_source_count_and_has_sources() {
        let state = BoardState::new("me", &[], &[]);
        assert_eq!(state.source_count(), 0);
        assert!(!state.has_sources());

        let boards = vec![create_test_board_config()];
        let state = BoardState::new("me", &boards, &[]);
        assert_eq!(state.source_count(), 1);
        assert!(state.has_sources());
    }

    #[test]
    fn test_clear_layout_cache() {
        let mut state = BoardState::new("me", &[], &[]);
        state
            .layout_cache
            .insert(("a".to_string(), "b".to_string(), "c".to_string()), vec![]);
        assert!(!state.layout_cache.is_empty());

        state.clear_layout_cache();
        assert!(state.layout_cache.is_empty());
    }
}
