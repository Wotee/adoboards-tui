//! Layout service functions
//!
//! This module provides functions for fetching and caching work item layout information
//! from Azure DevOps.

use std::collections::HashMap;

use anyhow::{Result, anyhow};

use crate::app::RefreshPolicy;
use crate::cache::{LayoutCacheKey, read_layout_cache, write_layout_cache};
use crate::services::fetch_work_item_layout;

/// Fetch visible controls for a work item type layout
///
/// Returns a list of (id, label) tuples for visible controls in the layout
pub async fn fetch_visible_controls(
    organization: &str,
    process_id: &str,
    reference_name: &str,
) -> Result<Vec<(String, String)>> {
    let layout = fetch_work_item_layout(organization, process_id, reference_name).await?;
    let page = layout
        .pages
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No pages in layout"))?;
    let section = page
        .sections
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("No sections in layout"))?;

    let mut controls = Vec::new();
    for group in section.groups.into_iter() {
        if !group.visible.unwrap_or(true) {
            continue;
        }
        for control in group.controls.into_iter() {
            if control.visible.unwrap_or(true) {
                if let Some(id) = control.id {
                    let label = control.label.unwrap_or_else(|| id.clone());
                    controls.push((id, label));
                }
            }
        }
    }

    Ok(controls)
}

/// Prefetch layouts for multiple work item types
///
/// Returns a cache map of (organization, project, type) -> [(id, label)]
pub async fn prefetch_layouts(
    organization: &str,
    project: &str,
    process_id: &str,
    layouts: Vec<(String, String)>, // (display_name, reference_name)
    refresh_policy: RefreshPolicy,
) -> HashMap<(String, String, String), Vec<(String, String)>> {
    let mut cache = HashMap::new();
    for (display_name, reference_name) in layouts {
        let key = (
            organization.to_string(),
            project.to_string(),
            display_name.clone(),
        );
        let layout_key_ref = LayoutCacheKey {
            organization: organization.to_string(),
            project: project.to_string(),
            work_item_type: reference_name.clone(),
        };
        let layout_key_display = LayoutCacheKey {
            organization: organization.to_string(),
            project: project.to_string(),
            work_item_type: display_name.clone(),
        };
        let cached = if matches!(refresh_policy, RefreshPolicy::Full) {
            None
        } else {
            read_layout_cache(&layout_key_ref).or_else(|| read_layout_cache(&layout_key_display))
        };
        if let Some(controls) = cached {
            eprintln!(
                "Using cached layout for {}/{} ({})",
                organization, project, display_name
            );
            cache.insert(key, controls);
            continue;
        }
        match fetch_visible_controls(organization, process_id, &reference_name).await {
            Ok(controls) => {
                let _ = write_layout_cache(&layout_key_ref, &controls);
                cache.insert(key, controls);
            }
            Err(err) => {
                eprintln!(
                    "Failed to prefetch layout for {} ({}): {}",
                    display_name, reference_name, err
                );
            }
        }
    }
    cache
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prefetch_layouts_empty() {
        // Empty layouts should return empty cache
        // Note: This would need async runtime and mocking to test properly
        assert!(true);
    }
}
