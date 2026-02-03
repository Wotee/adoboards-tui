use std::collections::{BTreeMap, HashMap};
use std::io;

use anyhow::{Result, anyhow};
use crossterm::event::KeyCode;
use ratatui::{Terminal, widgets::ListState};
use tokio::sync::oneshot;

use crate::cache::{LayoutCacheKey, read_layout_cache, write_layout_cache};
use crate::config::{AppConfig, BoardConfig, KeysConfig};
use crate::models::{DetailField, WorkItem};
pub use crate::picker::PickerState;
use crate::services::{WorkItemFieldInfo, fetch_work_item_layout, update_work_item_in_ado};
use crate::state::BoardState;


#[derive(Clone, PartialEq)]
pub enum RefreshPolicy {
    Normal,
    Full,
}

pub enum LoadingState {
    Loading,
    Loaded,
    Error(String),
}

// UI State types re-exported from ui_state module
pub use crate::ui_state::{DetailEditState, DetailViewState, ListViewState, SaveStatus, SourceEntry, VisibleField};

pub struct App {
    /// Business logic state (work items, sources, caches)
    pub board_state: BoardState,
    /// UI state for list view
    pub list_view_state: ListViewState,
    /// UI state for detail view
    pub detail_view_state: DetailViewState,
    /// Loading status
    pub loading_state: LoadingState,
    /// Current user name
    pub me: String,
    /// Key bindings configuration
    pub keys: KeysConfig,
    /// Last key pressed (for multi-key sequences)
    pub last_key_press: Option<KeyCode>,
    /// Refresh policy for data loading
    pub refresh_policy: RefreshPolicy,
    /// Whether help popup is showing
    pub showing_help: bool,
}

// Backward-compatible accessors for fields moved to board_state
impl App {
    /// Access work items
    pub fn items(&self) -> &Vec<WorkItem> {
        &self.board_state.items
    }

    /// Access sources
    pub fn sources(&self) -> &Vec<SourceEntry> {
        &self.board_state.sources
    }

    /// Access current source index
    pub fn current_source_index(&self) -> usize {
        self.board_state.current_source_index
    }

    /// Access work item types
    pub fn work_item_types(&self) -> &BTreeMap<String, String> {
        &self.board_state.work_item_types
    }

    /// Access process template type
    pub fn process_template_type(&self) -> Option<&String> {
        self.board_state.process_template_type.as_ref()
    }

    /// Access layout cache
    pub fn layout_cache(&self) -> &HashMap<(String, String, String), Vec<(String, String)>> {
        &self.board_state.layout_cache
    }

    /// Access field metadata cache
    pub fn field_meta_cache(&self) -> &HashMap<String, Vec<WorkItemFieldInfo>> {
        &self.board_state.field_meta_cache
    }
}

impl App {
    pub fn new(config: AppConfig) -> App {
        let mut list_state = ListState::default();
        let board_state = BoardState::new(&config.common.me, &config.boards, &config.iterations);

        if !board_state.sources.is_empty() {
            list_state.select(Some(0));
        }

        App {
            board_state,
            list_view_state: ListViewState::new(list_state),
            detail_view_state: DetailViewState::default(),
            loading_state: LoadingState::Loading,
            me: config.common.me,
            keys: config.keys,
            last_key_press: None,
            refresh_policy: RefreshPolicy::Normal,
            showing_help: false,
        }
    }

    pub fn set_work_item_types(&mut self, types: BTreeMap<String, String>) {
        self.board_state.work_item_types = types;
        self.clear_layout_cache();
        self.board_state.field_meta_cache.clear();
    }

    pub fn clear_layout_cache(&mut self) {
        self.board_state.layout_cache.clear();
    }

    pub fn set_initial_filter(&mut self, query: &str) {
        self.list_view_state.filter_query = query.to_string();
        self.clamp_selection();
    }

    pub fn set_process_template_type(&mut self, process_template_type: String) {
        self.board_state.process_template_type = Some(process_template_type);
        self.clear_layout_cache();
        self.board_state.field_meta_cache.clear();
    }

    pub fn current_source(&self) -> &SourceEntry {
        &self.board_state.sources[self.board_state.current_source_index]
    }

    pub fn load_data(&mut self, items: Vec<WorkItem>) {
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        self.list_view_state
            .type_picker
            .set_options(items.iter().map(|i| i.work_item_type.clone()));
        self.board_state.items = items;
        self.list_view_state.list_state = list_state;
        self.list_view_state.type_picker.selected = None;
        self.detail_view_state.edit_state = None;
        self.detail_view_state.save_status = SaveStatus::Idle;
        self.detail_view_state.save_receiver = None;
        self.loading_state = LoadingState::Loaded;
    }

    fn reset_inactive_edit_state(&mut self) {
        if let Some(state) = self.detail_view_state.edit_state.as_ref() {
            if !state.is_editing {
                self.detail_view_state.edit_state = None;
                self.detail_view_state.save_status = SaveStatus::Idle;
                self.detail_view_state.save_receiver = None;
            }
        }
    }

    pub fn jump_to_start(&mut self) {
        if !self.get_filtered_items().is_empty() {
            self.list_view_state.list_state.select(Some(0));
            self.reset_inactive_edit_state();
        }
    }

    pub fn jump_to_end(&mut self) {
        let items_len = self.get_filtered_items().len();
        if items_len > 0 {
            self.list_view_state.list_state.select(Some(items_len - 1));
            self.reset_inactive_edit_state();
        }
    }

    pub(crate) async fn ensure_detail_state_for_selected_item(&mut self) {
        if self.detail_view_state.edit_state.is_some() {
            return;
        }
        self.detail_view_state.save_status = SaveStatus::Idle;
        self.detail_view_state.save_receiver = None;
        if let Some(item) = self.get_selected_item().cloned() {
            let reference_name = self.board_state.work_item_types.get(&item.work_item_type).cloned();
            let mut edit_state = DetailEditState::new_from_item(&item);

            let organization = self.current_source().organization.clone();
            let project = self.current_source().project.clone();
            let cache_key = (
                organization.clone(),
                project.clone(),
                item.work_item_type.clone(),
            );
            let layout_key_display = LayoutCacheKey {
                organization: organization.clone(),
                project: project.clone(),
                work_item_type: item.work_item_type.clone(),
            };
            let layout_key_ref = reference_name.as_ref().map(|reference| LayoutCacheKey {
                organization: organization.clone(),
                project: project.clone(),
                work_item_type: reference.clone(),
            });

            let cached_controls = if self.refresh_policy == RefreshPolicy::Full {
                None
            } else if let Some(cached) = self.board_state.layout_cache.get(&cache_key) {
                Some(cached.clone())
            } else if let Some(disk) = read_layout_cache(&layout_key_display).or_else(|| {
                layout_key_ref
                    .as_ref()
                    .and_then(|ref_key| read_layout_cache(ref_key))
            }) {
                self.board_state.layout_cache.insert(cache_key.clone(), disk.clone());
                Some(disk)
            } else {
                None
            };

            let controls = if let Some(cached) = cached_controls {
                cached
            } else if let (Some(process_id), Some(reference)) =
                (self.board_state.process_template_type.clone(), reference_name.clone())
            {
                match fetch_visible_controls(&organization, &process_id, &reference).await {
                    Ok(controls) => {
                        if let Some(ref_key) = layout_key_ref.as_ref() {
                            let _ = write_layout_cache(ref_key, &controls);
                        }
                        let _ = write_layout_cache(&layout_key_display, &controls);
                        self.board_state.layout_cache.insert(cache_key.clone(), controls.clone());
                        controls
                    }
                    Err(err) => {
                        eprintln!("Failed to fetch layout: {}", err);
                        Vec::new()
                    }
                }
            } else {
                Vec::new()
            };

            let visible_fields = controls
                .into_iter()
                .filter_map(|(id, label)| {
                    item.fields.get(&id).cloned().map(|value| {
                        let allowed_values = self.board_state.field_meta_cache.get(&item.work_item_type).and_then(
                            |fields| {
                                fields
                                    .iter()
                                    .find(|f| f.reference_name == id)
                                    .map(|f| f.allowed_values.clone())
                            },
                        );
                        VisibleField::with_value(label, id, value, allowed_values)
                    })
                })
                .collect();
            edit_state.visible_fields = visible_fields;

            self.detail_view_state.edit_state = Some(edit_state);
            self.detail_view_state.save_status = SaveStatus::Idle;
            self.detail_view_state.save_receiver = None;
        }
    }

    pub fn toggle_type_filter_menu(&mut self) {
        self.list_view_state.type_picker.toggle_open();
        if self.list_view_state.type_picker.is_open {
        }
    }

    pub fn toggle_type_selection(&mut self) {
        if !self.list_view_state.type_picker.is_open {
            return;
        }

        self.list_view_state.type_picker.toggle_active();
        self.clamp_selection();
    }

    pub fn clear_type_filters(&mut self) {
        self.list_view_state.type_picker.clear_active();
        self.clamp_selection();
    }

    pub fn move_type_selection(&mut self, direction: isize) {
        if !self.list_view_state.type_picker.is_open {
            return;
        }

        self.list_view_state.type_picker.move_selection(direction);
    }

    pub fn open_item(&mut self) {
        let item = self.get_selected_item().unwrap();
        let source = self.current_source();
        let url = format!(
            "https://dev.azure.com/{}/{}/_workitems/edit/{}",
            source.organization, source.project, item.id,
        );

        if let Err(e) = open::that(url) {
            eprintln!("Failed to open link: {}", e);
        }
    }

    pub fn next_source(&mut self) {
        if self.board_state.sources.len() > 1 {
            self.board_state.current_source_index = (self.board_state.current_source_index + 1) % self.board_state.sources.len();
            self.loading_state = LoadingState::Loading;
        }
    }

    pub fn previous_source(&mut self) {
        if self.board_state.sources.len() > 1 {
            if self.board_state.current_source_index == 0 {
                self.board_state.current_source_index = self.board_state.sources.len() - 1;
            } else {
                self.board_state.current_source_index -= 1;
            }
            self.loading_state = LoadingState::Loading;
        }
    }

    pub fn get_selected_item(&self) -> Option<&WorkItem> {
        let selected_index = self.list_view_state.list_state.selected()?;
        self.get_filtered_items().get(selected_index).copied()
    }

    pub fn current_title(&self) -> String {
        self.current_source().title.clone()
    }

    pub fn clamp_selection(&mut self) {
        let item_count = self.get_filtered_items().len();

        if item_count == 0 {
            self.list_view_state.list_state.select(None);
            return;
        }

        if let Some(current_index) = self.list_view_state.list_state.selected() {
            if current_index >= item_count {
                self.list_view_state.list_state.select(Some(item_count - 1));
            }
        } else {
            self.list_view_state.list_state.select(Some(0));
        }
    }

    pub fn get_filtered_items(&self) -> Vec<&WorkItem> {
        self.board_state.items
            .iter()
            .filter(|item| {
                if self.list_view_state.assigned_to_me_filter_on {
                    if !item.assigned_to.contains(&self.me) {
                        return false;
                    }
                }

                if !self.list_view_state.type_picker.active.is_empty()
                    && !self
                        .list_view_state
                        .type_picker
                        .active
                        .contains(&item.work_item_type)
                {
                    return false;
                }

                if !self.list_view_state.filter_query.is_empty() {
                    let query = self.list_view_state.filter_query.to_lowercase();
                    let id_match = item.id.to_string().contains(&query);
                    let title_match = item.title.to_lowercase().contains(&query);
                    return id_match || title_match;
                }
                true
            })
            .collect()
    }

    pub fn toggle_assigned_to_me_filter(&mut self) {
        self.list_view_state.assigned_to_me_filter_on =
            !self.list_view_state.assigned_to_me_filter_on;
        self.list_view_state
            .list_state
            .select(self.get_filtered_items().first().map(|_| 0));
    }

    pub fn navigate_list(&mut self, direction: isize) {
        let count = self.get_filtered_items().len();
        if count == 0 {
            return;
        }
        let current = self.list_view_state.list_state.selected().unwrap_or(0) as isize;
        let next = (current + direction).clamp(0, count as isize - 1);
        self.list_view_state.list_state.select(Some(next as usize));
    }

    fn clamp_active_field(edit_state: &mut DetailEditState) {
        match edit_state.active_field {
            DetailField::Title => {}
            DetailField::Dynamic(idx) => {
                let total = edit_state.visible_fields.len();
                if total == 0 {
                    edit_state.active_field = DetailField::Title;
                } else if idx >= total {
                    edit_state.active_field = DetailField::Dynamic(total - 1);
                }
            }
        }
    }

    fn active_picker(edit_state: &DetailEditState) -> Option<&PickerState> {
        if let DetailField::Dynamic(idx) = edit_state.active_field {
            edit_state
                .visible_fields
                .get(idx)
                .and_then(|field| field.picker.as_ref())
        } else {
            None
        }
    }

    fn active_picker_mut(edit_state: &mut DetailEditState) -> Option<&mut PickerState> {
        if let DetailField::Dynamic(idx) = edit_state.active_field {
            edit_state
                .visible_fields
                .get_mut(idx)
                .and_then(|field| field.picker.as_mut())
        } else {
            None
        }
    }

    fn apply_active_picker_selection(edit_state: &mut DetailEditState) {
        if let DetailField::Dynamic(idx) = edit_state.active_field {
            if let Some(field) = edit_state.visible_fields.get_mut(idx) {
                if let Some(picker) = field.picker.as_mut() {
                    if let Some(selected) = picker.selected {
                        field.select_value(selected);
                    }
                }
            }
        }
    }

    fn rebuild_edit_state_from_item(
        item: &WorkItem,
        existing_fields: &[VisibleField],
    ) -> DetailEditState {
        let mut new_state = DetailEditState::new_from_item(item);
        new_state.visible_fields = existing_fields
            .iter()
            .map(|field| {
                let value = item
                    .fields
                    .get(&field.reference)
                    .cloned()
                    .unwrap_or_default();
                let allowed_values = field.picker.as_ref().map(|picker| picker.options.clone());
                VisibleField::with_value(
                    field.label.clone(),
                    field.reference.clone(),
                    value,
                    allowed_values,
                )
            })
            .collect();
        App::clamp_active_field(&mut new_state);
        new_state
    }

    pub(crate) fn cancel_edit(&mut self) {
        self.detail_view_state.save_receiver = None;
        if let Some(state) = self.detail_view_state.edit_state.as_ref() {
            if state.is_editing {
                self.detail_view_state.edit_state = None;
                self.detail_view_state.save_status = SaveStatus::Idle;
            }
        }
    }

    pub(crate) fn begin_edit(&mut self) {
        self.detail_view_state.save_receiver = None;
        self.detail_view_state.save_status = SaveStatus::Idle;
        
        if let Some(item) = self.get_selected_item() {
            let edit_state_exists = self.detail_view_state.edit_state.is_some();
            
            if edit_state_exists {
                // Update existing edit state
                let source = self.current_source();
                let cache_key = (
                    source.organization.clone(),
                    source.project.clone(),
                    item.work_item_type.clone(),
                );
                
                // Populate visible_fields with State, Assigned To, and layout fields
                let mut visible_fields = Vec::new();
                
                // Add State field with picker
                let state_allowed_values = self
                    .field_meta_cache()
                    .get(&item.work_item_type)
                    .and_then(|fields| {
                        fields
                            .iter()
                            .find(|f| f.reference_name == "System.State")
                            .map(|f| f.allowed_values.clone())
                    });
                let state_field = crate::ui_state::VisibleField::with_value(
                    "State".to_string(),
                    "System.State".to_string(),
                    item.state.clone(),
                    state_allowed_values,
                );
                visible_fields.push(state_field);
                
                // Add Assigned To field (no picker for now)
                let assigned_to_field = crate::ui_state::VisibleField::with_value(
                    "Assigned To".to_string(),
                    "System.AssignedTo".to_string(),
                    item.assigned_to.clone(),
                    None,
                );
                visible_fields.push(assigned_to_field);
                
                // Add other dynamic fields from layout
                if let Some(controls) = self.layout_cache().get(&cache_key) {
                    for (id, label) in controls {
                        if let Some(value) = item.fields.get(id) {
                            let allowed_values = self
                                .field_meta_cache()
                                .get(&item.work_item_type)
                                .and_then(|fields| {
                                    fields
                                        .iter()
                                        .find(|f| f.reference_name == *id)
                                        .map(|f| f.allowed_values.clone())
                                });
                            let field = crate::ui_state::VisibleField::with_value(
                                label.clone(),
                                id.clone(),
                                value.clone(),
                                allowed_values,
                            );
                            visible_fields.push(field);
                        }
                    }
                }
                
                if let Some(state) = self.detail_view_state.edit_state.as_mut() {
                    state.is_editing = true;
                    state.active_field = DetailField::Title;
                    state.visible_fields = visible_fields;
                    App::clamp_active_field(state);
                }
            } else {
                // Create new edit state
                let source = self.current_source();
                let cache_key = (
                    source.organization.clone(),
                    source.project.clone(),
                    item.work_item_type.clone(),
                );
                
                let mut state = DetailEditState::new_from_item(item);
                state.is_editing = true;
                
                // Add State field with picker
                let state_allowed_values = self
                    .field_meta_cache()
                    .get(&item.work_item_type)
                    .and_then(|fields| {
                        fields
                            .iter()
                            .find(|f| f.reference_name == "System.State")
                            .map(|f| f.allowed_values.clone())
                    });
                let state_field = crate::ui_state::VisibleField::with_value(
                    "State".to_string(),
                    "System.State".to_string(),
                    item.state.clone(),
                    state_allowed_values,
                );
                state.visible_fields.push(state_field);
                
                // Add Assigned To field (no picker for now)
                let assigned_to_field = crate::ui_state::VisibleField::with_value(
                    "Assigned To".to_string(),
                    "System.AssignedTo".to_string(),
                    item.assigned_to.clone(),
                    None,
                );
                state.visible_fields.push(assigned_to_field);
                
                // Add other dynamic fields from layout
                if let Some(controls) = self.layout_cache().get(&cache_key) {
                    for (id, label) in controls {
                        if let Some(value) = item.fields.get(id) {
                            let allowed_values = self
                                .field_meta_cache()
                                .get(&item.work_item_type)
                                .and_then(|fields| {
                                    fields
                                        .iter()
                                        .find(|f| f.reference_name == *id)
                                        .map(|f| f.allowed_values.clone())
                                });
                            let field = crate::ui_state::VisibleField::with_value(
                                label.clone(),
                                id.clone(),
                                value.clone(),
                                allowed_values,
                            );
                            state.visible_fields.push(field);
                        }
                    }
                }
                
                self.detail_view_state.edit_state = Some(state);
            }
        }
    }

    pub(crate) fn apply_typing(&mut self, c: char) {
        if let Some(state) = self.detail_view_state.edit_state.as_mut() {
            if !state.is_editing {
                return;
            }
            Self::clamp_active_field(state);
            match state.active_field {
                DetailField::Title => state.title.push(c),
                DetailField::Dynamic(idx) => {
                    if let Some(field) = state.visible_fields.get_mut(idx) {
                        let picker_has_options = field
                            .picker
                            .as_ref()
                            .map(|p| !p.options.is_empty())
                            .unwrap_or(false);
                        if !picker_has_options {
                            field.value.push(c);
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn apply_backspace(&mut self) {
        if let Some(state) = self.detail_view_state.edit_state.as_mut() {
            if !state.is_editing {
                return;
            }
            Self::clamp_active_field(state);
            match state.active_field {
                DetailField::Title => {
                    state.title.pop();
                }
                DetailField::Dynamic(idx) => {
                    if let Some(field) = state.visible_fields.get_mut(idx) {
                        let picker_has_options = field
                            .picker
                            .as_ref()
                            .map(|p| !p.options.is_empty())
                            .unwrap_or(false);
                        if !picker_has_options {
                            field.value.pop();
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn move_active_picker(&mut self, direction: isize) {
        if let Some(state) = self.detail_view_state.edit_state.as_mut() {
            if !state.is_editing {
                return;
            }
            Self::clamp_active_field(state);
            if let Some(picker) = App::active_picker_mut(state) {
                picker.move_selection(direction);
            }
        }
    }

    fn select_active_picker_value(&mut self) {
        if let Some(state) = self.detail_view_state.edit_state.as_mut() {
            if !state.is_editing {
                return;
            }
            Self::clamp_active_field(state);
            App::apply_active_picker_selection(state);
        }
    }

    pub(crate) fn start_save(&mut self) {
        let selected_item = self.get_selected_item().cloned();
        let source = self.current_source().clone();
        let state_for_save = self.detail_view_state.edit_state.clone();
        if let (Some(item), Some(save_state)) = (selected_item, state_for_save) {
            if !save_state.is_editing {
                return;
            }
            let (tx, rx) = oneshot::channel();
            tokio::spawn(async move {
                let result = update_work_item_in_ado(
                    &BoardConfig {
                        organization: source.organization,
                        project: source.project,
                        team: source.team,
                    },
                    &item,
                    &save_state,
                )
                .await
                .map(|_| (item, save_state));
                let _ = tx.send(result);
            });
            self.detail_view_state.save_status = SaveStatus::Saving;
            self.detail_view_state.save_receiver = Some(rx);
            if let Some(state) = self.detail_view_state.edit_state.as_mut() {
                state.is_editing = false;
            }
        }
    }

    pub(crate) fn poll_save_completion(&mut self) {
        if let Some(receiver) = self.detail_view_state.save_receiver.as_mut() {
            use tokio::sync::oneshot::error::TryRecvError;

            match receiver.try_recv() {
                Ok(Ok((updated_item, mut updated_state))) => {
                    if let Some(current_item) =
                        self.board_state.items.iter_mut().find(|i| i.id == updated_item.id)
                    {
                        current_item.title = updated_state.title.clone();
                        for field in &updated_state.visible_fields {
                            current_item
                                .fields
                                .insert(field.reference.clone(), field.value.clone());
                        }
                    }
                    updated_state.is_editing = false;
                    App::clamp_active_field(&mut updated_state);
                    self.detail_view_state.edit_state = Some(updated_state);
                    self.detail_view_state.save_status = SaveStatus::Idle;
                    self.detail_view_state.save_receiver = None;
                }
                Ok(Err(err)) => {
                    self.detail_view_state.save_status = SaveStatus::Failed(format!("{}", err));
                    self.detail_view_state.save_receiver = None;
                    if let Some(item) = self.get_selected_item().cloned() {
                        if let Some(state) = self.detail_view_state.edit_state.as_mut() {
                            let existing_fields = state.visible_fields.clone();
                            let reset = App::rebuild_edit_state_from_item(&item, &existing_fields);
                            *state = reset;
                        }
                    }
                }
                Err(TryRecvError::Closed) => {
                    self.detail_view_state.save_status =
                        SaveStatus::Failed("Save was cancelled".to_string());
                    self.detail_view_state.save_receiver = None;
                }
                Err(TryRecvError::Empty) => {}
            }
        }
    }
}

pub fn key_matches_sequence(
    current_key: char,
    last_key: Option<KeyCode>,
    target_sequence: &str,
) -> bool {
    if target_sequence.len() == 2 {
        let first_char = target_sequence.chars().next().unwrap();
        let second_char = target_sequence.chars().nth(1).unwrap();
        return last_key == Some(KeyCode::Char(first_char)) && current_key == second_char;
    }

    if target_sequence.len() == 1 {
        return target_sequence.chars().next() == Some(current_key);
    }

    false
}

async fn fetch_visible_controls(
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

pub async fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    use crate::input::EventLoop;
    EventLoop::new(terminal, app).run().await
}
