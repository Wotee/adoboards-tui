//! Filter logic for work items
//!
//! This module provides pure functions for filtering work items based on various criteria.
//! All functions are testable and have no side effects.

use std::collections::BTreeSet;

use crate::models::WorkItem;

/// Filter work items based on assigned user
pub fn filter_by_assigned_to<'a>(
    items: &'a [WorkItem],
    user: &'a str,
    enabled: bool,
) -> impl Iterator<Item = &'a WorkItem> + 'a {
    items.iter().filter(move |item| {
        if !enabled {
            return true;
        }
        item.assigned_to.contains(user)
    })
}

/// Filter work items based on type filters
pub fn filter_by_types<'a>(
    items: &'a [WorkItem],
    active_types: &'a BTreeSet<String>,
) -> impl Iterator<Item = &'a WorkItem> + 'a {
    items.iter().filter(move |item| {
        if active_types.is_empty() {
            return true;
        }
        active_types.contains(&item.work_item_type)
    })
}

/// Filter work items based on a text query (matches ID or title, case insensitive)
pub fn filter_by_query<'a>(
    items: &'a [WorkItem],
    query: &'a str,
) -> impl Iterator<Item = &'a WorkItem> + 'a {
    let query_lower = query.to_lowercase();
    items.iter().filter(move |item| {
        if query.is_empty() {
            return true;
        }
        let id_match = item.id.to_string().contains(&query_lower);
        let title_match = item.title.to_lowercase().contains(&query_lower);
        id_match || title_match
    })
}

/// Apply all filters to a list of work items
///
/// The filters are applied in this order:
/// 1. Assigned to me filter (if enabled)
/// 2. Type filter (if any types are selected)
/// 3. Text query filter (if query is not empty)
pub fn filter_items<'a>(
    items: &'a [WorkItem],
    assigned_to_user: &'a str,
    assigned_to_filter_enabled: bool,
    active_types: &'a BTreeSet<String>,
    query: &'a str,
) -> Vec<&'a WorkItem> {
    items
        .iter()
        .filter(|item| {
            // Assigned to filter
            if assigned_to_filter_enabled && !item.assigned_to.contains(assigned_to_user) {
                return false;
            }

            // Type filter
            if !active_types.is_empty() && !active_types.contains(&item.work_item_type) {
                return false;
            }

            // Query filter
            if !query.is_empty() {
                let query_lower = query.to_lowercase();
                let id_match = item.id.to_string().contains(&query_lower);
                let title_match = item.title.to_lowercase().contains(&query_lower);
                if !id_match && !title_match {
                    return false;
                }
            }

            true
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn create_test_item(id: u32, title: &str, item_type: &str, assigned_to: &str) -> WorkItem {
        WorkItem {
            id,
            title: title.to_string(),
            assigned_to: assigned_to.to_string(),
            state: "Active".to_string(),
            work_item_type: item_type.to_string(),
            description: String::new(),
            acceptance_criteria: String::new(),
            fields: BTreeMap::new(),
        }
    }

    #[test]
    fn test_filter_by_assigned_to_disabled_returns_all() {
        let items = vec![
            create_test_item(1, "Bug 1", "Bug", "Alice"),
            create_test_item(2, "Bug 2", "Bug", "Bob"),
        ];
        let filtered: Vec<_> = filter_by_assigned_to(&items, "Alice", false).collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_by_assigned_to_enabled_filters() {
        let items = vec![
            create_test_item(1, "Bug 1", "Bug", "Alice Smith"),
            create_test_item(2, "Bug 2", "Bug", "Bob Jones"),
            create_test_item(3, "Feature 1", "Feature", "Alice Smith"),
        ];
        let filtered: Vec<_> = filter_by_assigned_to(&items, "Alice", true).collect();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|i| i.assigned_to.contains("Alice")));
    }

    #[test]
    fn test_filter_by_types_empty_returns_all() {
        let items = vec![
            create_test_item(1, "Bug 1", "Bug", "Alice"),
            create_test_item(2, "Feature 1", "Feature", "Bob"),
        ];
        let active_types: BTreeSet<String> = BTreeSet::new();
        let filtered: Vec<_> = filter_by_types(&items, &active_types).collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_by_types_filters() {
        let items = vec![
            create_test_item(1, "Bug 1", "Bug", "Alice"),
            create_test_item(2, "Feature 1", "Feature", "Bob"),
            create_test_item(3, "Bug 2", "Bug", "Charlie"),
        ];
        let active_types: BTreeSet<String> = ["Bug".to_string()].into_iter().collect();
        let filtered: Vec<_> = filter_by_types(&items, &active_types).collect();
        assert_eq!(filtered.len(), 2);
        assert!(filtered.iter().all(|i| i.work_item_type == "Bug"));
    }

    #[test]
    fn test_filter_by_types_multiple_types() {
        let items = vec![
            create_test_item(1, "Bug 1", "Bug", "Alice"),
            create_test_item(2, "Feature 1", "Feature", "Bob"),
            create_test_item(3, "Task 1", "Task", "Charlie"),
        ];
        let active_types: BTreeSet<String> = ["Bug".to_string(), "Feature".to_string()]
            .into_iter()
            .collect();
        let filtered: Vec<_> = filter_by_types(&items, &active_types).collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_by_query_empty_returns_all() {
        let items = vec![
            create_test_item(1, "Bug 1", "Bug", "Alice"),
            create_test_item(2, "Feature 1", "Feature", "Bob"),
        ];
        let filtered: Vec<_> = filter_by_query(&items, "").collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_by_query_matches_title() {
        let items = vec![
            create_test_item(1, "Login bug", "Bug", "Alice"),
            create_test_item(2, "User profile", "Feature", "Bob"),
            create_test_item(3, "Logout issue", "Bug", "Charlie"),
        ];
        let filtered: Vec<_> = filter_by_query(&items, "login").collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 1);
    }

    #[test]
    fn test_filter_by_query_matches_id() {
        let items = vec![
            create_test_item(1234, "Bug 1", "Bug", "Alice"),
            create_test_item(5678, "Bug 2", "Bug", "Bob"),
        ];
        let filtered: Vec<_> = filter_by_query(&items, "123").collect();
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 1234);
    }

    #[test]
    fn test_filter_by_query_case_insensitive() {
        let items = vec![create_test_item(1, "Login Bug", "Bug", "Alice")];
        let filtered: Vec<_> = filter_by_query(&items, "LOGIN").collect();
        assert_eq!(filtered.len(), 1);
    }

    #[test]
    fn test_filter_by_query_matches_partial_id() {
        let items = vec![
            create_test_item(1001, "First", "Bug", "Alice"),
            create_test_item(1002, "Second", "Bug", "Bob"),
            create_test_item(2001, "Third", "Bug", "Charlie"),
        ];
        let filtered: Vec<_> = filter_by_query(&items, "100").collect();
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_items_combined_filters() {
        let items = vec![
            create_test_item(1, "Login bug", "Bug", "Alice Smith"),
            create_test_item(2, "Login feature", "Feature", "Bob Jones"),
            create_test_item(3, "Logout bug", "Bug", "Alice Smith"),
        ];

        // Filter by: assigned to Alice, type Bug, query "login"
        let active_types: BTreeSet<String> = ["Bug".to_string()].into_iter().collect();
        let filtered = filter_items(
            &items,
            "Alice",
            true, // assigned to me filter on
            &active_types,
            "login",
        );

        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].id, 1);
    }

    #[test]
    fn test_filter_items_no_filters_returns_all() {
        let items = vec![
            create_test_item(1, "Bug 1", "Bug", "Alice"),
            create_test_item(2, "Feature 1", "Feature", "Bob"),
        ];
        let active_types: BTreeSet<String> = BTreeSet::new();
        let filtered = filter_items(&items, "Alice", false, &active_types, "");
        assert_eq!(filtered.len(), 2);
    }

    #[test]
    fn test_filter_items_empty_input() {
        let items: Vec<WorkItem> = vec![];
        let active_types: BTreeSet<String> = ["Bug".to_string()].into_iter().collect();
        let filtered = filter_items(&items, "Alice", true, &active_types, "query");
        assert!(filtered.is_empty());
    }
}
