use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::time::Duration;

use anyhow::{Result, anyhow};
use crossterm::event::{self, Event, KeyCode};
use ratatui::{Terminal, widgets::ListState};
use tokio::sync::oneshot;

use crate::cache::{LayoutCacheKey, read_layout_cache, write_layout_cache};
use crate::config::{AppConfig, BoardConfig, IterationConfig, KeysConfig};
use crate::models::{DetailField, WorkItem};
use crate::services::{WorkItemFieldInfo, fetch_work_item_layout, update_work_item_in_ado};
use crate::ui::{draw_detail_view, draw_list_view, draw_status_screen};

pub enum AppView {
    List,
    Detail,
}

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

#[derive(Clone, Default)]
pub struct PickerState {
    pub is_open: bool,
    pub options: Vec<String>,
    pub selected: Option<usize>,
    pub active: BTreeSet<String>,
}

impl PickerState {
    pub fn from_options(options: Vec<String>) -> Self {
        let mut state = Self::default();
        state.set_options(options);
        state
    }

    pub fn set_options<I: IntoIterator<Item = String>>(&mut self, options: I) {
        let unique: BTreeSet<String> = options.into_iter().collect();
        self.options = unique.into_iter().collect();
        self.clamp_selection();
    }

    pub fn toggle_open(&mut self) {
        self.is_open = !self.is_open;
        if self.is_open {
            self.clamp_selection();
        } else {
            self.selected = None;
        }
    }

    pub fn close(&mut self) {
        self.is_open = false;
        self.selected = None;
    }

    pub fn move_selection(&mut self, direction: isize) {
        if self.options.is_empty() {
            self.selected = None;
            return;
        }
        let current = self.selected.unwrap_or(0) as isize;
        let next = (current + direction).clamp(0, self.options.len() as isize - 1);
        self.selected = Some(next as usize);
    }

    pub fn toggle_active(&mut self) {
        if let Some(idx) = self.selected {
            if let Some(value) = self.options.get(idx).cloned() {
                if self.active.contains(&value) {
                    self.active.remove(&value);
                } else {
                    self.active.insert(value);
                }
            }
        }
    }

    pub fn clear_active(&mut self) {
        self.active.clear();
    }

    pub fn set_selected_to_value(&mut self, value: &str) {
        self.selected = self.options.iter().position(|v| v == value);
    }

    fn clamp_selection(&mut self) {
        if self.options.is_empty() {
            self.selected = None;
            return;
        }
        let max_idx = self.options.len() - 1;
        let selection = self.selected.unwrap_or(0).min(max_idx);
        self.selected = Some(selection);
    }
}

pub struct ListViewState {
    pub list_state: ListState,
    pub filter_query: String,
    pub is_filtering: bool,
    pub is_list_details_hover_visible: bool,
    pub assigned_to_me_filter_on: bool,
    pub type_picker: PickerState,
}

impl ListViewState {
    pub fn new(list_state: ListState) -> Self {
        Self {
            list_state,
            filter_query: String::new(),
            is_filtering: false,
            is_list_details_hover_visible: false,
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
        let mut picker = allowed_values.map(PickerState::from_options);
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

    fn select_value(&mut self, idx: usize) {
        if let Some(picker) = self.picker.as_mut() {
            if let Some(choice) = picker.options.get(idx).cloned() {
                self.value = choice;
                picker.selected = Some(idx);
            }
        }
    }
}

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

#[derive(Default)]
pub struct DetailViewState {
    pub edit_state: Option<DetailEditState>,
    pub save_status: SaveStatus,
    pub save_receiver: Option<oneshot::Receiver<Result<(WorkItem, DetailEditState)>>>,
}

#[derive(Clone)]
pub enum SourceKind {
    Backlog,
    Iteration(IterationConfig),
}

#[derive(Default, Clone)]
pub enum SaveStatus {
    #[default]
    Idle,
    Saving,
    Failed(String),
}

#[derive(Clone)]
pub struct SourceEntry {
    pub title: String,
    pub team: String,
    pub organization: String,
    pub project: String,
    pub kind: SourceKind,
}

pub struct App {
    pub view: AppView,
    pub items: Vec<WorkItem>,
    pub list_view_state: ListViewState,
    pub detail_view_state: DetailViewState,
    pub loading_state: LoadingState,
    pub sources: Vec<SourceEntry>,
    pub current_source_index: usize,
    pub me: String,
    pub keys: KeysConfig,
    pub last_key_press: Option<KeyCode>,
    pub work_item_types: BTreeMap<String, String>,
    pub process_template_type: Option<String>,
    pub layout_cache: HashMap<(String, String, String), Vec<(String, String)>>,
    pub field_meta_cache: HashMap<String, Vec<WorkItemFieldInfo>>,
    pub refresh_policy: RefreshPolicy,
}

impl App {
    pub fn new(config: AppConfig) -> App {
        let mut list_state = ListState::default();
        let mut sources: Vec<SourceEntry> = Vec::new();

        for board in &config.boards {
            sources.push(SourceEntry {
                title: format!("{} Backlog", board.team),
                team: board.team.clone(),
                organization: board.organization.clone(),
                project: board.project.clone(),
                kind: SourceKind::Backlog,
            });
        }

        for iteration in &config.iterations {
            sources.push(SourceEntry {
                title: format!("{} Iteration: {}", iteration.team, iteration.iteration),
                team: iteration.team.clone(),
                organization: iteration.organization.clone(),
                project: iteration.project.clone(),
                kind: SourceKind::Iteration(iteration.clone()),
            });
        }

        if !sources.is_empty() {
            list_state.select(Some(0));
        }

        App {
            view: AppView::List,
            items: Vec::new(),
            list_view_state: ListViewState::new(list_state),
            detail_view_state: DetailViewState::default(),
            loading_state: LoadingState::Loading,
            sources,
            current_source_index: 0,
            me: config.common.me,
            keys: config.keys,
            last_key_press: None,
            work_item_types: BTreeMap::new(),
            process_template_type: None,
            layout_cache: HashMap::new(),
            field_meta_cache: HashMap::new(),
            refresh_policy: RefreshPolicy::Normal,
        }
    }

    pub fn set_work_item_types(&mut self, types: BTreeMap<String, String>) {
        self.work_item_types = types;
        self.clear_layout_cache();
        self.field_meta_cache.clear();
    }

    pub fn clear_layout_cache(&mut self) {
        self.layout_cache.clear();
    }

    pub fn set_process_template_type(&mut self, process_template_type: String) {
        self.process_template_type = Some(process_template_type);
        self.clear_layout_cache();
        self.field_meta_cache.clear();
    }

    pub fn current_source(&self) -> &SourceEntry {
        &self.sources[self.current_source_index]
    }

    pub fn load_data(&mut self, items: Vec<WorkItem>) {
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        self.list_view_state
            .type_picker
            .set_options(items.iter().map(|i| i.work_item_type.clone()));
        self.items = items;
        self.list_view_state.list_state = list_state;
        self.list_view_state.type_picker.selected = None;
        self.loading_state = LoadingState::Loaded;
    }

    pub fn jump_to_start(&mut self) {
        if !self.get_filtered_items().is_empty() {
            self.list_view_state.list_state.select(Some(0));
        }
    }

    pub fn jump_to_end(&mut self) {
        let items_len = self.get_filtered_items().len();
        if items_len > 0 {
            self.list_view_state.list_state.select(Some(items_len - 1));
        }
    }

    pub fn toggle_type_filter_menu(&mut self) {
        self.list_view_state.type_picker.toggle_open();
        if self.list_view_state.type_picker.is_open {
            self.list_view_state.is_list_details_hover_visible = false;
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
        if self.sources.len() > 1 {
            self.current_source_index = (self.current_source_index + 1) % self.sources.len();
            self.loading_state = LoadingState::Loading;
        }
    }

    pub fn previous_source(&mut self) {
        if self.sources.len() > 1 {
            if self.current_source_index == 0 {
                self.current_source_index = self.sources.len() - 1;
            } else {
                self.current_source_index -= 1;
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
        self.items
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
        self.list_view_state.is_list_details_hover_visible = false;
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

    fn cancel_edit(&mut self) {
        self.detail_view_state.save_receiver = None;
        let selected_item = self.get_selected_item().cloned();
        if let Some(state) = self.detail_view_state.edit_state.as_mut() {
            if state.is_editing {
                let existing_fields = state.visible_fields.clone();
                if let Some(item) = selected_item {
                    let new_state = App::rebuild_edit_state_from_item(&item, &existing_fields);
                    *state = new_state;
                }
                state.is_editing = false;
                self.detail_view_state.save_status = SaveStatus::Idle;
            } else {
                self.view = AppView::List;
            }
        } else {
            self.view = AppView::List;
        }
    }

    fn begin_edit(&mut self) {
        self.detail_view_state.save_receiver = None;
        self.detail_view_state.save_status = SaveStatus::Idle;
        if let Some(state) = self.detail_view_state.edit_state.as_mut() {
            state.is_editing = true;
            state.active_field = DetailField::Title;
            App::clamp_active_field(state);
        } else if let Some(item) = self.get_selected_item() {
            let mut state = DetailEditState::new_from_item(item);
            state.is_editing = true;
            self.detail_view_state.edit_state = Some(state);
        }
    }

    fn apply_typing(&mut self, c: char) {
        if let Some(state) = self.detail_view_state.edit_state.as_mut() {
            if !state.is_editing {
                return;
            }
            Self::clamp_active_field(state);
            match state.active_field {
                DetailField::Title => state.title.push(c),
                DetailField::Dynamic(idx) => {
                    if let Some(field) = state.visible_fields.get_mut(idx) {
                        if field.picker.is_none() {
                            field.value.push(c);
                        }
                    }
                }
            }
        }
    }

    fn move_active_picker(&mut self, direction: isize) {
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

    fn start_save(&mut self) {
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

    fn poll_save_completion(&mut self) {
        if let Some(receiver) = self.detail_view_state.save_receiver.as_mut() {
            use tokio::sync::oneshot::error::TryRecvError;

            match receiver.try_recv() {
                Ok(Ok((updated_item, mut updated_state))) => {
                    if let Some(current_item) =
                        self.items.iter_mut().find(|i| i.id == updated_item.id)
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
                // Write under both reference name (backward compatibility) and display name (primary)
                let _ = write_layout_cache(&layout_key_ref, &controls);
                let _ = write_layout_cache(&layout_key_display, &controls);
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
    if matches!(app.loading_state, LoadingState::Loading) {
        return Ok(());
    }
    loop {
        terminal.draw(|f| match app.loading_state {
            LoadingState::Loaded => match app.view {
                AppView::List => draw_list_view(f, app),
                AppView::Detail => draw_detail_view(f, app),
            },
            LoadingState::Loading => {}
            LoadingState::Error(ref msg) => {
                draw_status_screen(f, &format!("Failed to load data. {}", msg))
            }
        })?;

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.loading_state {
                    LoadingState::Loading | LoadingState::Error(_) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        _ => {}
                    },
                    _ => {
                        if app.list_view_state.is_filtering {
                            match key.code {
                                KeyCode::Enter | KeyCode::Esc => {
                                    app.list_view_state.is_filtering = false;
                                    if key.code == KeyCode::Esc {
                                        app.list_view_state.filter_query.clear();
                                        app.clamp_selection();
                                    }
                                }
                                KeyCode::Backspace => {
                                    app.list_view_state.filter_query.pop();
                                    app.clamp_selection();
                                }
                                KeyCode::Char(c) => {
                                    if c != '/' {
                                        app.list_view_state.filter_query.push(c);
                                        app.clamp_selection();
                                    }
                                }
                                _ => {}
                            }
                        } else if app.list_view_state.type_picker.is_open {
                            match key.code {
                                KeyCode::Esc => {
                                    app.list_view_state.type_picker.close();
                                }
                                KeyCode::Char('c') => {
                                    app.clear_type_filters();
                                    app.list_view_state.type_picker.close();
                                }
                                KeyCode::Enter | KeyCode::Char(' ') => {
                                    app.toggle_type_selection();
                                }
                                KeyCode::Up => {
                                    app.move_type_selection(-1);
                                }
                                KeyCode::Down => {
                                    app.move_type_selection(1);
                                }
                                KeyCode::Char(c) => {
                                    let last_key = app.last_key_press;
                                    if key_matches_sequence(c, last_key, &app.keys.quit) {
                                        app.list_view_state.type_picker.close();
                                        app.last_key_press = None;
                                    } else if key_matches_sequence(c, last_key, &app.keys.next) {
                                        app.move_type_selection(1);
                                        app.last_key_press = Some(key.code);
                                    } else if key_matches_sequence(c, last_key, &app.keys.previous)
                                    {
                                        app.move_type_selection(-1);
                                        app.last_key_press = Some(key.code);
                                    } else {
                                        app.last_key_press = None;
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            let current_char = match key.code {
                                KeyCode::Char(c) => Some(c),
                                _ => None,
                            };
                            match app.view {
                                AppView::List => {
                                    if let Some(c) = current_char {
                                        let last_key = app.last_key_press;

                                        if key_matches_sequence(c, last_key, &app.keys.jump_to_top)
                                        {
                                            app.list_view_state.is_list_details_hover_visible =
                                                false;
                                            app.jump_to_start();
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.jump_to_end,
                                        ) {
                                            app.list_view_state.is_list_details_hover_visible =
                                                false;
                                            app.jump_to_end();
                                        } else if key_matches_sequence(c, last_key, &app.keys.quit)
                                        {
                                            return Ok(());
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.search,
                                        ) {
                                            app.list_view_state.is_list_details_hover_visible =
                                                false;
                                            app.list_view_state.is_filtering = true;
                                            app.list_view_state.filter_query.clear();
                                            app.clamp_selection();
                                        } else if key_matches_sequence(c, last_key, &app.keys.next)
                                        {
                                            app.list_view_state.is_list_details_hover_visible =
                                                false;
                                            app.navigate_list(1);
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.previous,
                                        ) {
                                            app.list_view_state.is_list_details_hover_visible =
                                                false;
                                            app.navigate_list(-1);
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.next_board,
                                        ) {
                                            app.list_view_state.is_list_details_hover_visible =
                                                false;
                                            app.next_source();
                                            return Ok(());
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.previous_board,
                                        ) {
                                            app.list_view_state.is_list_details_hover_visible =
                                                false;
                                            app.previous_source();
                                            return Ok(());
                                        } else if key_matches_sequence(c, last_key, &app.keys.hover)
                                        {
                                            app.list_view_state.is_list_details_hover_visible =
                                                true;
                                        } else if key_matches_sequence(c, last_key, &app.keys.open)
                                        {
                                            app.open_item();
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.assigned_to_me_filter,
                                        ) {
                                            app.toggle_assigned_to_me_filter()
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.work_item_type_filter,
                                        ) {
                                            app.toggle_type_filter_menu();
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.refresh,
                                        ) {
                                            app.refresh_policy = RefreshPolicy::Normal;
                                            app.loading_state = LoadingState::Loading;
                                            return Ok(());
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.full_refresh,
                                        ) {
                                            app.refresh_policy = RefreshPolicy::Full;
                                            app.loading_state = LoadingState::Loading;
                                            return Ok(());
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.edit_config,
                                        ) {
                                            let _ = crate::config::open_config();
                                            eprintln!(
                                                "Reopen adoboards for changes to take effect"
                                            );
                                            return Ok(());
                                        }

                                        app.last_key_press = Some(key.code);
                                    } else {
                                        match key.code {
                                            KeyCode::Enter => {
                                                app.list_view_state.is_list_details_hover_visible =
                                                    false;
                                                if app
                                                    .list_view_state
                                                    .list_state
                                                    .selected()
                                                    .is_some()
                                                {
                                                    app.view = AppView::Detail;
                                                    if let Some(item) =
                                                        app.get_selected_item().cloned()
                                                    {
                                                        let reference_name = app
                                                            .work_item_types
                                                            .get(&item.work_item_type)
                                                            .cloned();
                                                        let mut edit_state =
                                                            DetailEditState::new_from_item(&item);
                                                        if let (Some(process_id), Some(reference)) = (
                                                            app.process_template_type.clone(),
                                                            reference_name,
                                                        ) {
                                                            let organization = app
                                                                .current_source()
                                                                .organization
                                                                .clone();
                                                            let project = app
                                                                .current_source()
                                                                .project
                                                                .clone();
                                                             let cache_key = (
                                                                 organization.clone(),
                                                                 project.clone(),
                                                                 item.work_item_type.clone(),
                                                             );
                                                             let layout_key_ref = LayoutCacheKey {
                                                                 organization: organization.clone(),
                                                                 project: project.clone(),
                                                                 work_item_type: reference.clone(),
                                                             };
                                                             let layout_key_display = LayoutCacheKey {
                                                                 organization: organization.clone(),
                                                                 project: project.clone(),
                                                                 work_item_type: item
                                                                     .work_item_type
                                                                     .clone(),
                                                             };
 
                                                             let cached_controls = if app
                                                                 .refresh_policy
                                                                 == RefreshPolicy::Full
                                                             {
                                                                 None
                                                             } else if let Some(cached) =
                                                                 app.layout_cache.get(&cache_key)
                                                             {
                                                                 Some(cached.clone())
                                                             } else if let Some(disk) = read_layout_cache(
                                                                 &layout_key_display,
                                                             )
                                                             .or_else(|| read_layout_cache(&layout_key_ref))
                                                             {
                                                                 app.layout_cache.insert(
                                                                     cache_key.clone(),
                                                                     disk.clone(),
                                                                 );
                                                                 Some(disk)
                                                             } else {
                                                                 None
                                                             };
 
                                                             let controls = if let Some(cached) =
                                                                 cached_controls
                                                             {
                                                                 cached
                                                             } else {
                                                                 match fetch_visible_controls(
                                                                     &organization,
                                                                     &process_id,
                                                                     &reference,
                                                                 )
                                                                 .await
                                                                 {
                                                                     Ok(controls) => {
                                                                         let _ = write_layout_cache(
                                                                             &layout_key_ref,
                                                                             &controls,
                                                                         );
                                                                         let _ = write_layout_cache(
                                                                             &layout_key_display,
                                                                             &controls,
                                                                         );
                                                                         app.layout_cache.insert(
                                                                             cache_key.clone(),
                                                                             controls.clone(),
                                                                         );
                                                                         controls
                                                                     }
                                                                     Err(err) => {
                                                                         eprintln!(
                                                                             "Failed to fetch layout: {}",
                                                                             err
                                                                         );
                                                                         Vec::new()
                                                                     }
                                                                 }
                                                             };


                                                            let visible_fields = controls
                                                                .into_iter()
                                                                .filter_map(|(id, label)| {
                                                                    item.fields
                                                 .get(&id)

                                                 .cloned()
                                                 .map(|value| {
                                                     let allowed_values = app
                                                         .field_meta_cache
                                                         .get(&item.work_item_type)
                                                         .and_then(|fields| {
                                                             fields
                                                                 .iter()
                                                                 .find(|f| f.reference_name == id)
                                                                 .map(|f| f.allowed_values.clone())
                                                         });
                                                     VisibleField::with_value(
                                                         label,
                                                         id,
                                                         value,
                                                         allowed_values,
                                                     )
                                                 })
                                                                })
                                                                .collect();
                                                            edit_state.visible_fields =
                                                                visible_fields;
                                                        }

                                                        app.detail_view_state.edit_state =
                                                            Some(edit_state);
                                                        app.detail_view_state.save_status =
                                                            SaveStatus::Idle;
                                                        app.detail_view_state.save_receiver = None;
                                                    }
                                                }
                                            }
                                            KeyCode::Esc => {
                                                if app.list_view_state.assigned_to_me_filter_on {
                                                    app.toggle_assigned_to_me_filter()
                                                }
                                                app.list_view_state.is_list_details_hover_visible =
                                                    false;
                                                if !app.list_view_state.filter_query.is_empty() {
                                                    app.list_view_state.filter_query.clear();
                                                    app.clamp_selection();
                                                }
                                                if app.list_view_state.type_picker.is_open {
                                                    app.toggle_type_filter_menu();
                                                }
                                            }

                                            KeyCode::Up => {
                                                app.list_view_state.is_list_details_hover_visible =
                                                    false;
                                                app.navigate_list(-1);
                                            }
                                            KeyCode::Down => {
                                                app.list_view_state.is_list_details_hover_visible =
                                                    false;
                                                app.navigate_list(1);
                                            }
                                            _ => {}
                                        }
                                        app.last_key_press = None;
                                    }
                                }
                                AppView::Detail => {
                                    app.poll_save_completion();

                                    if matches!(
                                        app.detail_view_state.save_status,
                                        SaveStatus::Saving
                                    ) {
                                        app.last_key_press = None;
                                        continue;
                                    }

                                    if let Some(c) = current_char {
                                        if let Some(state) =
                                            app.detail_view_state.edit_state.as_mut()
                                        {
                                            if state.is_editing {
                                                App::clamp_active_field(state);
                                                if App::active_picker(state).is_some() {
                                                    let last_key = app.last_key_press;
                                                    if key_matches_sequence(
                                                        c,
                                                        last_key,
                                                        &app.keys.next,
                                                    ) {
                                                        app.move_active_picker(1);
                                                    } else if key_matches_sequence(
                                                        c,
                                                        last_key,
                                                        &app.keys.previous,
                                                    ) {
                                                        app.move_active_picker(-1);
                                                    }
                                                    app.last_key_press = Some(key.code);
                                                    continue;
                                                }

                                                app.apply_typing(c);
                                                app.last_key_press = None;
                                                continue;
                                            }
                                        }

                                        let last_key = app.last_key_press;

                                        if key_matches_sequence(c, last_key, &app.keys.quit) {
                                            app.view = AppView::List;
                                        } else if key_matches_sequence(c, last_key, &app.keys.open)
                                        {
                                            app.open_item();
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.edit_item,
                                        ) {
                                            app.begin_edit();
                                        }

                                        app.last_key_press = Some(key.code);
                                    } else {
                                        match key.code {
                                            KeyCode::Esc => {
                                                app.cancel_edit();
                                            }
                                            KeyCode::Tab => {
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        let total_fields =
                                                            state.visible_fields.len();
                                                        let next = match state.active_field {
                                                            DetailField::Title => {
                                                                if total_fields == 0 {
                                                                    DetailField::Title
                                                                } else {
                                                                    DetailField::Dynamic(0)
                                                                }
                                                            }
                                                            DetailField::Dynamic(idx) => {
                                                                if idx + 1 < total_fields {
                                                                    DetailField::Dynamic(idx + 1)
                                                                } else {
                                                                    DetailField::Title
                                                                }
                                                            }
                                                        };
                                                        state.active_field = next;
                                                        App::clamp_active_field(state);
                                                    }
                                                }
                                            }
                                            KeyCode::BackTab => {
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        let total_fields =
                                                            state.visible_fields.len();
                                                        let prev = match state.active_field {
                                                            DetailField::Title => {
                                                                if total_fields == 0 {
                                                                    DetailField::Title
                                                                } else {
                                                                    DetailField::Dynamic(
                                                                        total_fields - 1,
                                                                    )
                                                                }
                                                            }
                                                            DetailField::Dynamic(idx) => {
                                                                if idx == 0 {
                                                                    DetailField::Title
                                                                } else {
                                                                    DetailField::Dynamic(idx - 1)
                                                                }
                                                            }
                                                        };
                                                        state.active_field = prev;
                                                        App::clamp_active_field(state);
                                                    }
                                                }
                                            }
                                            KeyCode::Up => {
                                                app.move_active_picker(-1);
                                            }
                                            KeyCode::Down => {
                                                app.move_active_picker(1);
                                            }
                                            KeyCode::Enter => {
                                                app.select_active_picker_value();
                                                app.start_save();
                                            }

                                            KeyCode::Delete => {
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        App::clamp_active_field(state);
                                                        match state.active_field {
                                                            DetailField::Title => {
                                                                state.title.clear()
                                                            }
                                                            DetailField::Dynamic(idx) => {
                                                                if let Some(field) = state
                                                                    .visible_fields
                                                                    .get_mut(idx)
                                                                {
                                                                    if field.picker.is_none() {
                                                                        field.value.clear();
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            KeyCode::Backspace => {
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        App::clamp_active_field(state);
                                                        match state.active_field {
                                                            DetailField::Title => {
                                                                state.title.pop();
                                                            }
                                                            DetailField::Dynamic(idx) => {
                                                                if let Some(field) = state
                                                                    .visible_fields
                                                                    .get_mut(idx)
                                                                {
                                                                    if field.picker.is_none() {
                                                                        field.value.pop();
                                                                    }
                                                                }
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {}
                                        }
                                        app.last_key_press = None;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}
