use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::APPNAME;
use crate::models::WorkItem;
use crate::services::WorkItemFieldInfo;

#[derive(Clone, Debug)]
pub enum WorkItemsCacheKey {
    Backlog {
        organization: String,
        project: String,
        team: String,
    },
    Iteration {
        organization: String,
        project: String,
        team: String,
        iteration: String,
    },
}

#[derive(Clone, Debug)]
pub struct LayoutCacheKey {
    pub organization: String,
    pub project: String,
    pub work_item_type: String,
}

#[derive(Clone, Debug)]
pub struct FieldMetaCacheKey {
    pub organization: String,
    pub project: String,
    pub work_item_type: String,
}

#[derive(Serialize, Deserialize)]
struct WorkItemsCacheEntry {
    updated_at: u64,
    items: Vec<WorkItem>,
}

#[derive(Serialize, Deserialize)]
struct LayoutCacheEntry {
    updated_at: u64,
    controls: Vec<LayoutControlEntry>,
}

#[derive(Serialize, Deserialize)]
struct FieldMetaCacheEntry {
    updated_at: u64,
    fields: Vec<WorkItemFieldInfo>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct LayoutControlEntry {
    pub id: String,
    pub label: String,
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

fn sanitize_component(input: &str) -> String {
    input
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect()
}

fn cache_root() -> Result<PathBuf> {
    let config_file = confy::get_configuration_file_path(APPNAME, None)?;
    let config_dir = config_file
        .parent()
        .ok_or_else(|| anyhow!("Configuration path has no parent"))?;
    Ok(config_dir.join("cache"))
}

fn work_items_cache_path(key: &WorkItemsCacheKey) -> Result<PathBuf> {
    let base = cache_root()?.join("work_items");
    let name = match key {
        WorkItemsCacheKey::Backlog {
            organization,
            project,
            team,
        } => format!(
            "backlog_{}_{}_{}.json",
            sanitize_component(organization),
            sanitize_component(project),
            sanitize_component(team)
        ),
        WorkItemsCacheKey::Iteration {
            organization,
            project,
            team,
            iteration,
        } => format!(
            "iteration_{}_{}_{}_{}.json",
            sanitize_component(organization),
            sanitize_component(project),
            sanitize_component(team),
            sanitize_component(iteration)
        ),
    };
    Ok(base.join(name))
}

fn layout_cache_path(key: &LayoutCacheKey) -> Result<PathBuf> {
    let base = cache_root()?.join("layout");
    let name = format!(
        "layout_{}_{}_{}.json",
        sanitize_component(&key.organization),
        sanitize_component(&key.project),
        sanitize_component(&key.work_item_type)
    );
    Ok(base.join(name))
}

fn field_meta_cache_path(key: &FieldMetaCacheKey) -> Result<PathBuf> {
    let base = cache_root()?.join("field_meta");
    let name = format!(
        "fieldmeta_{}_{}_{}.json",
        sanitize_component(&key.organization),
        sanitize_component(&key.project),
        sanitize_component(&key.work_item_type)
    );
    Ok(base.join(name))
}

fn is_fresh(updated_at: u64, max_age: Duration) -> bool {
    if let Some(updated) = UNIX_EPOCH.checked_add(Duration::from_secs(updated_at)) {
        if let Ok(elapsed) = SystemTime::now().duration_since(updated) {
            return elapsed <= max_age;
        }
    }
    false
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create cache directory: {}", parent.display()))?;
    }
    Ok(())
}

pub fn read_work_items_cache(key: &WorkItemsCacheKey, max_age: Duration) -> Option<Vec<WorkItem>> {
    let path = match work_items_cache_path(key) {
        Ok(p) => p,
        Err(_) => {
            return None;
        }
    };
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            return None;
        }
    };
    let entry: WorkItemsCacheEntry = match serde_json::from_slice(&data) {
        Ok(v) => v,
        Err(_) => {
            return None;
        }
    };
    if is_fresh(entry.updated_at, max_age) {
        Some(entry.items)
    } else {
        None
    }
}

pub fn write_work_items_cache(key: &WorkItemsCacheKey, items: &[WorkItem]) -> Result<()> {
    let path = work_items_cache_path(key)?;
    ensure_parent_dir(&path)?;
    let entry = WorkItemsCacheEntry {
        updated_at: now_secs(),
        items: items.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&entry)?;
    fs::write(&path, json)
        .with_context(|| format!("Failed to write work item cache: {}", path.display()))?;
    Ok(())
}

pub fn read_layout_cache(key: &LayoutCacheKey) -> Option<Vec<(String, String)>> {
    let path = match layout_cache_path(key) {
        Ok(p) => p,
        Err(_) => {
            return None;
        }
    };
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            return None;
        }
    };
    let entry: LayoutCacheEntry = match serde_json::from_slice(&data) {
        Ok(v) => v,
        Err(_) => {
            return None;
        }
    };
    let controls = entry
        .controls
        .into_iter()
        .map(|c| (c.id, c.label))
        .collect();
    Some(controls)
}

pub fn write_layout_cache(key: &LayoutCacheKey, controls: &[(String, String)]) -> Result<()> {
    let path = layout_cache_path(key)?;
    ensure_parent_dir(&path)?;
    let entry = LayoutCacheEntry {
        updated_at: now_secs(),
        controls: controls
            .iter()
            .cloned()
            .map(|(id, label)| LayoutControlEntry { id, label })
            .collect(),
    };
    let json = serde_json::to_vec_pretty(&entry)?;
    fs::write(&path, json)
        .with_context(|| format!("Failed to write layout cache: {}", path.display()))?;
    Ok(())
}

pub fn read_field_meta_cache(key: &FieldMetaCacheKey) -> Option<Vec<WorkItemFieldInfo>> {
    let path = match field_meta_cache_path(key) {
        Ok(p) => p,
        Err(_) => {
            return None;
        }
    };
    let data = match fs::read(&path) {
        Ok(d) => d,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return None,
        Err(_) => {
            return None;
        }
    };
    let entry: FieldMetaCacheEntry = match serde_json::from_slice(&data) {
        Ok(v) => v,
        Err(_) => {
            return None;
        }
    };
    Some(entry.fields)
}

pub fn write_field_meta_cache(key: &FieldMetaCacheKey, fields: &[WorkItemFieldInfo]) -> Result<()> {
    let path = field_meta_cache_path(key)?;
    ensure_parent_dir(&path)?;
    let entry = FieldMetaCacheEntry {
        updated_at: now_secs(),
        fields: fields.to_vec(),
    };
    let json = serde_json::to_vec_pretty(&entry)?;
    fs::write(&path, json)
        .with_context(|| format!("Failed to write field meta cache: {}", path.display()))?;
    Ok(())
}
