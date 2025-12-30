use crate::config::{AppConfig, BoardConfig, KeysConfig};
use crate::models::{DetailField, WorkItem};
use crate::services::update_work_item_in_ado;
use crate::ui::{draw_detail_view, draw_list_view, draw_status_screen};
use crossterm::event::{self, Event, KeyCode};
use ratatui::{Terminal, widgets::ListState};
use std::{collections::BTreeSet, io, time::Duration};

pub enum AppView {
    List,
    Detail,
}

pub enum LoadingState {
    Loading,
    Loaded,
    Error(String),
}

pub struct ListViewState {
    pub list_state: ListState,
    pub filter_query: String,
    pub is_filtering: bool,
    pub is_list_details_hover_visible: bool,
    pub assigned_to_me_filter_on: bool,
    pub is_type_filter_open: bool,
    pub active_type_filters: BTreeSet<String>,
    pub available_types: BTreeSet<String>,
    pub type_filter_selection: Option<usize>,
}

impl ListViewState {
    pub fn new(list_state: ListState) -> Self {
        Self {
            list_state,
            filter_query: String::new(),
            is_filtering: false,
            is_list_details_hover_visible: false,
            assigned_to_me_filter_on: false,
            is_type_filter_open: false,
            active_type_filters: BTreeSet::new(),
            available_types: BTreeSet::new(),
            type_filter_selection: None,
        }
    }
}

impl Default for ListViewState {
    fn default() -> Self {
        Self::new(ListState::default())
    }
}

#[derive(Clone)]
pub struct DetailEditState {
    pub is_editing: bool,
    pub active_field: DetailField,
    pub title: String,
    pub description: String,
    pub acceptance_criteria: String,
}

impl DetailEditState {
    pub fn new_from_item(item: &WorkItem) -> Self {
        Self {
            is_editing: false,
            active_field: DetailField::Title,
            title: item.title.clone(),
            description: item.description.clone(),
            acceptance_criteria: item.acceptance_criteria.clone(),
        }
    }
}

#[derive(Default)]
pub struct DetailViewState {
    pub edit_state: Option<DetailEditState>,
}

pub struct App {
    pub view: AppView,
    pub items: Vec<WorkItem>,
    pub list_view_state: ListViewState,
    pub detail_view_state: DetailViewState,
    pub loading_state: LoadingState,
    pub all_boards: Vec<BoardConfig>,
    pub current_board_index: usize,
    pub me: String,
    pub keys: KeysConfig,
    pub last_key_press: Option<KeyCode>,
}

impl App {
    pub fn new(config: AppConfig) -> App {
        let mut list_state = ListState::default();
        if !config.boards.is_empty() {
            list_state.select(Some(0));
        }
        App {
            view: AppView::List,
            items: Vec::new(),
            list_view_state: ListViewState::new(list_state),
            detail_view_state: DetailViewState::default(),
            loading_state: LoadingState::Loading,
            all_boards: config.boards,
            current_board_index: 0,
            me: config.common.me,
            keys: config.keys,
            last_key_press: None,
        }
    }

    pub fn current_board(&self) -> &BoardConfig {
        &self.all_boards[self.current_board_index]
    }

    pub fn load_data(&mut self, items: Vec<WorkItem>) {
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        self.list_view_state.available_types =
            items.iter().map(|i| i.work_item_type.clone()).collect();
        self.items = items;
        self.list_view_state.list_state = list_state;
        self.list_view_state.type_filter_selection = None;
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
        self.list_view_state.is_type_filter_open = !self.list_view_state.is_type_filter_open;
        if self.list_view_state.is_type_filter_open {
            let available_len = self.list_view_state.available_types.len();
            self.list_view_state.type_filter_selection =
                if available_len > 0 { Some(0) } else { None };
            self.list_view_state.is_list_details_hover_visible = false;
        } else {
            self.list_view_state.type_filter_selection = None;
        }
    }

    pub fn toggle_type_selection(&mut self) {
        if !self.list_view_state.is_type_filter_open {
            return;
        }

        if let Some(selected_index) = self.list_view_state.type_filter_selection {
            if let Some(selected_type) = self
                .list_view_state
                .available_types
                .iter()
                .nth(selected_index)
                .cloned()
            {
                if self
                    .list_view_state
                    .active_type_filters
                    .contains(&selected_type)
                {
                    self.list_view_state
                        .active_type_filters
                        .remove(&selected_type);
                } else {
                    self.list_view_state
                        .active_type_filters
                        .insert(selected_type);
                }
                self.clamp_selection();
            }
        }
    }

    pub fn clear_type_filters(&mut self) {
        self.list_view_state.active_type_filters.clear();
        self.clamp_selection();
    }

    pub fn move_type_selection(&mut self, direction: isize) {
        if !self.list_view_state.is_type_filter_open {
            return;
        }

        let len = self.list_view_state.available_types.len();
        if len == 0 {
            self.list_view_state.type_filter_selection = None;
            return;
        }

        let current = self.list_view_state.type_filter_selection.unwrap_or(0) as isize;
        let next = (current + direction).clamp(0, len as isize - 1);
        self.list_view_state.type_filter_selection = Some(next as usize);
    }

    pub fn open_item(&mut self) {
        let board = self.all_boards.get(self.current_board_index).unwrap();
        let item = self.get_selected_item().unwrap();
        let url = format!(
            "https://dev.azure.com/{}/{}/_workitems/edit/{}",
            board.organization, board.project, item.id,
        );

        if let Err(e) = open::that(url) {
            eprintln!("Failed to open link: {}", e);
        }
    }

    pub fn next_board(&mut self) {
        if self.all_boards.len() > 1 {
            self.current_board_index = (self.current_board_index + 1) % self.all_boards.len();
            self.loading_state = LoadingState::Loading;
        }
    }

    pub fn previous_board(&mut self) {
        if self.all_boards.len() > 1 {
            if self.current_board_index == 0 {
                self.current_board_index = self.all_boards.len() - 1;
            } else {
                self.current_board_index -= 1;
            }
            self.loading_state = LoadingState::Loading;
        }
    }

    pub fn get_selected_item(&self) -> Option<&WorkItem> {
        let selected_index = self.list_view_state.list_state.selected()?;
        self.get_filtered_items().get(selected_index).copied()
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

                if !self.list_view_state.active_type_filters.is_empty()
                    && !self
                        .list_view_state
                        .active_type_filters
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
                        } else if app.list_view_state.is_type_filter_open {
                            match key.code {
                                KeyCode::Esc => {
                                    app.list_view_state.is_type_filter_open = false;
                                    app.list_view_state.type_filter_selection = None;
                                }
                                KeyCode::Char('c') => {
                                    app.clear_type_filters();
                                    app.list_view_state.is_type_filter_open = false;
                                    app.list_view_state.type_filter_selection = None;
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
                                        app.list_view_state.is_type_filter_open = false;
                                        app.list_view_state.type_filter_selection = None;
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
                                            app.next_board();
                                            return Ok(());
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.previous_board,
                                        ) {
                                            app.list_view_state.is_list_details_hover_visible =
                                                false;
                                            app.previous_board();
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
                                                    if let Some(item) = app.get_selected_item() {
                                                        app.detail_view_state.edit_state = Some(
                                                            DetailEditState::new_from_item(item),
                                                        );
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
                                                if app.list_view_state.is_type_filter_open {
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
                                    if let Some(c) = current_char {
                                        if let Some(state) =
                                            app.detail_view_state.edit_state.as_mut()
                                        {
                                            if state.is_editing {
                                                match state.active_field {
                                                    DetailField::Title => state.title.push(c),
                                                    DetailField::Description => {
                                                        state.description.push(c)
                                                    }
                                                    DetailField::AcceptanceCriteria => {
                                                        state.acceptance_criteria.push(c)
                                                    }
                                                }
                                                app.last_key_press = None;
                                                continue;
                                            }
                                        }

                                        let last_key = app.last_key_press;

                                        if key_matches_sequence(c, last_key, &app.keys.quit) {
                                            app.view = AppView::List
                                        }
                                        if key_matches_sequence(c, last_key, &app.keys.open) {
                                            app.open_item()
                                        }
                                        if key_matches_sequence(c, last_key, &app.keys.edit_item) {
                                            if let Some(item) = app.get_selected_item() {
                                                app.detail_view_state.edit_state =
                                                    Some(DetailEditState::new_from_item(item));
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    state.is_editing = true;
                                                }
                                            }
                                        }
                                        app.last_key_press = Some(key.code);
                                    } else {
                                        match key.code {
                                            KeyCode::Esc => {
                                                let selected_item =
                                                    app.get_selected_item().cloned();
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        if let Some(item) = selected_item {
                                                            *state = DetailEditState::new_from_item(
                                                                &item,
                                                            );
                                                        }
                                                        state.is_editing = false;
                                                    } else {
                                                        app.view = AppView::List;
                                                    }
                                                } else {
                                                    app.view = AppView::List;
                                                }
                                            }
                                            KeyCode::Tab => {
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        state.active_field =
                                                            match state.active_field {
                                                                DetailField::Title => {
                                                                    DetailField::Description
                                                                }
                                                                DetailField::Description => {
                                                                    DetailField::AcceptanceCriteria
                                                                }
                                                                DetailField::AcceptanceCriteria => {
                                                                    DetailField::Title
                                                                }
                                                            };
                                                    }
                                                }
                                            }
                                            KeyCode::BackTab => {
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        state.active_field =
                                                            match state.active_field {
                                                                DetailField::Title => {
                                                                    DetailField::AcceptanceCriteria
                                                                }
                                                                DetailField::Description => {
                                                                    DetailField::Title
                                                                }
                                                                DetailField::AcceptanceCriteria => {
                                                                    DetailField::Description
                                                                }
                                                            };
                                                    }
                                                }
                                            }
                                            KeyCode::Enter => {
                                                let selected_item =
                                                    app.get_selected_item().cloned();
                                                let board = app.current_board().clone();
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        if let Some(item) = selected_item {
                                                            let local_state = state.clone();
                                                            let item_for_spawn = item.clone();
                                                            tokio::spawn(async move {
                                                                if let Err(err) =
                                                                    update_work_item_in_ado(
                                                                        &board,
                                                                        &item_for_spawn,
                                                                        &local_state,
                                                                    )
                                                                    .await
                                                                {
                                                                    eprintln!(
                                                                        "Failed to update item: {:?}",
                                                                        err
                                                                    );
                                                                }
                                                            });
                                                            if let Some(current_item) = app
                                                                .items
                                                                .iter_mut()
                                                                .find(|i| i.id == item.id)
                                                            {
                                                                current_item.title =
                                                                    state.title.clone();
                                                                current_item.description =
                                                                    state.description.clone();
                                                                current_item.acceptance_criteria =
                                                                    state
                                                                        .acceptance_criteria
                                                                        .clone();
                                                            }
                                                        }
                                                        state.is_editing = false;
                                                    }
                                                }
                                            }
                                            KeyCode::Char(c) => {
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        match state.active_field {
                                                            DetailField::Title => {
                                                                state.title.push(c)
                                                            }
                                                            DetailField::Description => {
                                                                state.description.push(c)
                                                            }
                                                            DetailField::AcceptanceCriteria => {
                                                                state.acceptance_criteria.push(c)
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            KeyCode::Delete => {
                                                if let Some(state) =
                                                    app.detail_view_state.edit_state.as_mut()
                                                {
                                                    if state.is_editing {
                                                        match state.active_field {
                                                            DetailField::Title => {
                                                                state.title.clear()
                                                            }
                                                            DetailField::Description => {
                                                                state.description.clear()
                                                            }
                                                            DetailField::AcceptanceCriteria => {
                                                                state.acceptance_criteria.clear()
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
                                                        match state.active_field {
                                                            DetailField::Title => {
                                                                state.title.pop();
                                                            }
                                                            DetailField::Description => {
                                                                state.description.pop();
                                                            }
                                                            DetailField::AcceptanceCriteria => {
                                                                state.acceptance_criteria.pop();
                                                            }
                                                        }
                                                    }
                                                }
                                            }
                                            _ => {}
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
}
