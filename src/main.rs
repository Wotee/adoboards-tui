use anyhow::Result;
use azure_devops_rust_api::Credential;
use azure_devops_rust_api::wit::ClientBuilder as WitClientBuilder;
use azure_devops_rust_api::wit::models::WorkItem as ADOWorkItem;
use azure_devops_rust_api::work::ClientBuilder as WorkClientBuilder;
use azure_identity::AzureCliCredential;
use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use html_escape::decode_html_entities;
use lazy_static::lazy_static;
use ratatui::{
    Terminal,
    backend::CrosstermBackend,
    layout::{Constraint, Direction, Layout, Position, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph, Wrap},
};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::{error::Error, io, time::Duration};

// --- Data Model Structs ---

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BoardConfig {
    organization: String,
    project: String,
    team: String,
}

#[derive(Default, Clone, Debug, Deserialize, Serialize)]
pub struct CommonConfig {
    me: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(default)]
pub struct KeysConfig {
    quit: String,
    next: String,
    previous: String,
    hover: String,
    open: String,
    next_board: String,
    previous_board: String,
    search: String,
    assigned_to_me_filter: String,
    jump_to_top: String,
    jump_to_end: String,
}

impl Default for KeysConfig {
    fn default() -> Self {
        KeysConfig {
            quit: "q".to_string(),
            next: "j".to_string(),
            previous: "k".to_string(),
            hover: "K".to_string(),
            open: "o".to_string(),
            next_board: ">".to_string(),
            previous_board: "<".to_string(),
            search: "/".to_string(),
            assigned_to_me_filter: "m".to_string(),
            jump_to_top: "gg".to_string(),
            jump_to_end: "G".to_string(),
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    common: CommonConfig,
    #[serde(default)]
    boards: Vec<BoardConfig>,
    #[serde(default)]
    keys: KeysConfig,
}

/// Represents a simple work item returned by the API.
#[derive(Clone, Debug)]
pub struct WorkItem {
    id: u32,
    title: String,
    assigned_to: String,
    state: String,
    work_item_type: String,
    description: String,
    acceptance_criteria: String,
}

impl From<ADOWorkItem> for WorkItem {
    fn from(item: ADOWorkItem) -> Self {
        let get_and_clean_field = |key: &str| -> String {
            item.fields
                .get(key)
                .and_then(|v| v.as_str())
                .map_or("".to_string(), clean_ado_text)
        };
        let assigned_to_name: String = item
            .fields
            .get("System.AssignedTo")
            .and_then(|assigned_to| assigned_to.as_object())
            .and_then(|assigned_to| assigned_to.get("displayName"))
            .and_then(|display_name| display_name.as_str())
            .map(|s| s.to_string())
            .unwrap_or("Unassigned".to_string());

        WorkItem {
            id: item.id as u32,
            title: get_and_clean_field("System.Title"),
            work_item_type: get_and_clean_field("System.WorkItemType"),
            description: get_and_clean_field("System.Description"),
            acceptance_criteria: get_and_clean_field("Microsoft.VSTS.Common.AcceptanceCriteria"),
            assigned_to: assigned_to_name,
            state: get_and_clean_field("System.State"),
        }
    }
}

lazy_static! {
    /// Regex to strip common HTML tags (like <p>, <div>, <span>, <img>, etc.)
    static ref HTML_TAG_REGEX: Regex = Regex::new(r"<[^>]*>").unwrap();
}

fn clean_ado_text(input: &str) -> String {
    let decoded_text = decode_html_entities(input).to_string();
    let stripped_text = HTML_TAG_REGEX.replace_all(&decoded_text, "").to_string();
    stripped_text.trim().to_string()
}

fn authenticate_with_cli_credential() -> Result<Credential> {
    let azure_cli_credential = AzureCliCredential::new(None)?;
    Ok(Credential::from_token_credential(azure_cli_credential))
}

fn get_credential() -> Result<Credential> {
    match std::env::var("ADO_TOKEN") {
        Ok(token) if !token.is_empty() => {
            println!("Authenticate using PAT provided via $ADO_TOKEN");
            Ok(Credential::from_pat(token))
        }
        _ => authenticate_with_cli_credential(),
    }
}

pub async fn get_backlog(
    organization: &str,
    project: &str,
    team: &str,
) -> Result<Vec<WorkItem>, Box<dyn Error>> {
    let credential = get_credential()?;
    let work_client = WorkClientBuilder::new(credential.clone()).build();

    // Black magic string
    let backlog_level = "Microsoft.RequirementCategory";

    let backlog_result = work_client
        .backlogs_client()
        .get_backlog_level_work_items(organization, project, team, backlog_level)
        .await?;

    let work_item_ids: Vec<i32> = backlog_result
        .work_items
        .into_iter()
        .filter_map(|wi_link| wi_link.target)
        .filter_map(|wi| wi.id)
        .collect();

    if work_item_ids.is_empty() {
        println!(
            "No work items found on the backlog level '{}'",
            backlog_level
        );
        return Ok(Vec::new());
    }

    let ids: String = work_item_ids
        .into_iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");

    let wit_client = WitClientBuilder::new(credential).build();

    let full_items = wit_client
        .work_items_client()
        .list(organization, ids, project)
        .await?;

    let items = full_items.value.into_iter().map(WorkItem::from).collect();
    Ok(items)
}

// --- Application State ---

/// Defines which view the application is currently showing.
enum AppView {
    List,
    Detail,
}

enum LoadingState {
    Loading,
    Loaded,
    Error(String),
}

struct App {
    view: AppView,
    items: Vec<WorkItem>,
    list_state: ListState,
    loading_state: LoadingState,
    filter_query: String,
    is_filtering: bool,
    is_list_details_hover_visible: bool,
    all_boards: Vec<BoardConfig>,
    current_board_index: usize,
    me: String,
    assigned_to_me_filter_on: bool,
    keys: KeysConfig,
    // To support sequences like 'gg'
    last_key_press: Option<KeyCode>,
}

impl App {
    fn new(config: AppConfig) -> App {
        let mut list_state = ListState::default();
        if !config.boards.is_empty() {
            list_state.select(Some(0));
        }
        App {
            view: AppView::List,
            items: Vec::new(),
            list_state: ListState::default(),
            loading_state: LoadingState::Loading,
            filter_query: String::new(),
            is_filtering: false,
            is_list_details_hover_visible: false,
            all_boards: config.boards,
            current_board_index: 0,
            me: config.common.me,
            assigned_to_me_filter_on: false,
            keys: config.keys,
            last_key_press: None,
        }
    }

    fn jump_to_start(&mut self) {
        if !self.get_filtered_items().is_empty() {
            self.list_state.select(Some(0));
        }
    }

    fn jump_to_end(&mut self) {
        let items_len = self.get_filtered_items().len();
        if items_len > 0 {
            self.list_state.select(Some(items_len - 1));
        }
    }

    fn current_board(&self) -> &BoardConfig {
        &self.all_boards[self.current_board_index]
    }

    fn open_item(&mut self) {
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

    fn next_board(&mut self) {
        if self.all_boards.len() > 1 {
            self.current_board_index = (self.current_board_index + 1) % self.all_boards.len();
            self.loading_state = LoadingState::Loading;
        }
    }

    fn previous_board(&mut self) {
        if self.all_boards.len() > 1 {
            if self.current_board_index == 0 {
                self.current_board_index = self.all_boards.len() - 1;
            } else {
                self.current_board_index -= 1;
            }
            self.loading_state = LoadingState::Loading;
        }
    }

    fn get_selected_item(&self) -> Option<&WorkItem> {
        let selected_index = self.list_state.selected()?;
        self.get_filtered_items().get(selected_index).copied()
    }

    fn clamp_selection(&mut self) {
        let item_count = self.get_filtered_items().len();

        if item_count == 0 {
            self.list_state.select(None);
            return;
        }

        if let Some(current_index) = self.list_state.selected() {
            if current_index >= item_count {
                self.list_state.select(Some(item_count - 1));
            }
        } else {
            self.list_state.select(Some(0));
        }
    }

    fn get_filtered_items(&self) -> Vec<&WorkItem> {
        self.items
            .iter()
            .filter(|item| {
                // Apply assigned to me filter first
                if self.assigned_to_me_filter_on {
                    if !item.assigned_to.contains(&self.me) {
                        return false;
                    }
                }
                // Apply the text search filter
                if !self.filter_query.is_empty() {
                    let query = self.filter_query.to_lowercase();
                    let id_match = item.id.to_string().contains(&query);
                    let title_match = item.title.to_lowercase().contains(&query);
                    return id_match || title_match;
                }
                true
            })
            .collect()
    }

    fn toggle_assigned_to_me_filter(&mut self) {
        self.assigned_to_me_filter_on = !self.assigned_to_me_filter_on;
        self.is_list_details_hover_visible = false;
        self.list_state
            .select(self.get_filtered_items().first().map(|_| 0));
    }

    fn load_data(&mut self, items: Vec<WorkItem>) {
        let mut list_state = ListState::default();
        if !items.is_empty() {
            list_state.select(Some(0));
        }
        self.items = items;
        self.list_state = list_state;
        self.loading_state = LoadingState::Loaded;
    }

    fn navigate_list(&mut self, direction: isize) {
        let count = self.get_filtered_items().len();
        if count == 0 {
            return;
        }
        let current = self.list_state.selected().unwrap_or(0) as isize;
        let next = (current + direction).clamp(0, count as isize - 1);
        self.list_state.select(Some(next as usize));
    }
}

fn load_config() -> Result<AppConfig> {
    let config_content = std::fs::read_to_string("config.toml")?;
    let app_config: AppConfig = toml::from_str(&config_content)?;
    if app_config.boards.is_empty() {
        return Err(anyhow::anyhow!(
            "Configuration file 'config.toml' is missing any board definitions."
        ));
    }
    if app_config.common.me.is_empty() {
        return Err(anyhow::anyhow!(
            "Configuration file 'config.toml' is missing common.me"
        ));
    }
    Ok(app_config)
}

// --- TUI Drawing Functions ---
fn calculate_popup_rect(frame_area: Rect, app: &App, list_area: Rect) -> Option<Rect> {
    let selected_index = app.list_state.selected()?;
    let offset = app.list_state.offset();

    let relative_y = (selected_index.saturating_sub(offset)) as u16;

    let popup_height = 4;
    let popup_width = 45;

    let selected_y_on_screen = list_area.y + 1 + relative_y;

    let mut x = list_area.x + 20;
    let mut y = selected_y_on_screen + 1;

    if y + popup_height > frame_area.height {
        y = selected_y_on_screen.saturating_sub(popup_height);
    }

    y = y.max(frame_area.y);

    if x + popup_width > frame_area.width {
        x = frame_area.width.saturating_sub(popup_width + 1);
    }
    x = x.max(frame_area.x + 1);
    Some(Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    })
}

fn draw_hover_popup(f: &mut ratatui::Frame, app: &mut App, list_area: Rect) {
    if app.is_list_details_hover_visible {
        if let Some(item) = app.get_selected_item() {
            if let Some(popup_rect) = calculate_popup_rect(f.area(), app, list_area) {
                f.render_widget(Clear, popup_rect);
                let content_text = vec![
                    Line::from(format!("Assigned To: {}", item.assigned_to)),
                    Line::from(format!("State: {}", item.state)),
                ];

                let popup_block = Block::default()
                    .borders(Borders::ALL)
                    .title("Details")
                    .border_style(Style::default().fg(Color::LightBlue));
                f.render_widget(Paragraph::new(content_text).block(popup_block), popup_rect);
            }
        }
    }
}

fn draw_list_view(f: &mut ratatui::Frame, app: &mut App) {
    let constraints = if app.is_filtering {
        [Constraint::Min(0), Constraint::Length(3)]
    } else {
        [Constraint::Min(0), Constraint::Length(0)]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(constraints.as_ref())
        .split(f.area());

    let items_to_display = app.get_filtered_items();

    let list_items: Vec<ListItem> = items_to_display
        .iter()
        .map(|item| {
            let content = Line::from(format!("{}: {}", item.id, item.title));
            ListItem::new(content).style(Style::default().fg(Color::White))
        })
        .collect();

    let board_title: String = if app.assigned_to_me_filter_on {
        format!(
            "{} Backlog, Assigned to {}",
            app.current_board().team,
            app.me,
        )
    } else {
        format!("{} Backlog", app.current_board().team)
    };

    let list = List::new(list_items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .border_style(Color::LightBlue)
                .title(board_title),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    let list_area = chunks[0];
    f.render_stateful_widget(list, chunks[0], &mut app.list_state);

    draw_hover_popup(f, app, list_area);

    if app.is_filtering {
        let filter_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::LightBlue))
            .title("Filter Mode");

        let filter_text = Line::from(format!("/{}", app.filter_query));
        let filter_paragraph = Paragraph::new(filter_text).block(filter_block);
        f.render_widget(filter_paragraph, chunks[1]);

        // Set the cursor position for input
        let x = chunks[1].x + 2 + app.filter_query.len() as u16;
        let y = chunks[1].y + 1;
        f.set_cursor_position(Position::new(x, y));
    }
}

/// Renders the Detail View for the selected work item.
fn draw_detail_view(f: &mut ratatui::Frame, app: &App) {
    let filtered_items = app.get_filtered_items();
    let selected_index = app.list_state.selected().unwrap_or(0);
    let item = filtered_items.get(selected_index).expect(
        "Logic Error: Detail view opened but item selection is invalid for the filtered list.",
    );

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints(
            [
                Constraint::Length(3), // Title
                Constraint::Min(0),    // Description
                Constraint::Length(5), // Acceptance Criteria
            ]
            .as_ref(),
        )
        .split(f.area());

    let title_text = format!("{}: {}", item.id, item.title);
    let title_block = Block::default()
        .title(item.work_item_type.to_string())
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightBlue));
    let title_paragraph = Paragraph::new(title_text)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(title_block);
    f.render_widget(title_paragraph, chunks[0]);

    let desc_block = Block::default().title("Description").borders(Borders::ALL);
    let desc_paragraph = Paragraph::new(item.description.clone())
        .wrap(Wrap { trim: false })
        .block(desc_block);
    f.render_widget(desc_paragraph, chunks[1]);

    let ac_block = Block::default()
        .title("Acceptance Criteria")
        .borders(Borders::ALL);
    let ac_paragraph = Paragraph::new(item.acceptance_criteria.clone())
        .wrap(Wrap { trim: false })
        .block(ac_block);
    f.render_widget(ac_paragraph, chunks[2]);
}

fn draw_status_screen(f: &mut ratatui::Frame, message: &str) {
    let area = f.area();
    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Color::LightBlue)
        .title("Status");
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            message,
            Style::default().add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from("Press 'q' to quit."),
    ];

    let paragraph = Paragraph::new(text)
        .alignment(ratatui::layout::Alignment::Center)
        .wrap(Wrap { trim: false })
        .block(block);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Percentage(40),
                Constraint::Length(5),
                Constraint::Percentage(40),
            ]
            .as_ref(),
        )
        .split(area);

    f.render_widget(paragraph, chunks[1]);
}

// --- Main Loop and Setup ---
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let all_boards = match load_config() {
        Ok(boards) => boards,
        Err(e) => {
            eprintln!("Configuration Error: {}", e);
            return Err(e.into());
        }
    };

    let mut app = App::new(all_boards);
    let mut res = Ok(());

    while !matches!(app.loading_state, LoadingState::Error(_)) {
        if matches!(app.loading_state, LoadingState::Loading) {
            let board = app.current_board();
            let board_title = format!("{} Backlog", board.team);
            terminal.draw(|f| draw_status_screen(f, &format!("Loading {}...", board_title)))?;

            let fetch_result = get_backlog(&board.organization, &board.project, &board.team).await;
            match fetch_result {
                Ok(items) => {
                    app.load_data(items);
                }
                Err(e) => {
                    let error_msg = format!("Failed to fetch data: {:?}", e);
                    eprintln!("\n--- FATAL FETCH ERROR ---\n{}", error_msg);
                    app.loading_state = LoadingState::Error(error_msg);
                }
            }
            continue;
        }
        res = run_app(&mut terminal, &mut app);
        if res.is_err() {
            break;
        }

        if matches!(app.loading_state, LoadingState::Loading) {
            continue;
        }
        break;
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    // Handle any errors from the main loop
    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}

fn key_matches_sequence(
    current_key: char,
    last_key: Option<KeyCode>,
    target_sequence: &str,
) -> bool {
    if target_sequence.len() == 1 {
        return target_sequence.chars().next() == Some(current_key);
    }

    if target_sequence.len() == 2 {
        let first_char = target_sequence.chars().next().unwrap();
        let second_char = target_sequence.chars().nth(1).unwrap();
        return last_key == Some(KeyCode::Char(first_char)) && current_key == second_char;
    }

    // No longer combinations supported for now
    false
}

fn run_app<B: ratatui::backend::Backend>(
    terminal: &mut Terminal<B>,
    app: &mut App,
) -> io::Result<()> {
    if matches!(app.loading_state, LoadingState::Loading) {
        return Ok(());
    }
    loop {
        // Draw the UI based on the current view
        terminal.draw(|f| match app.loading_state {
            LoadingState::Loaded => match app.view {
                AppView::List => draw_list_view(f, app),
                AppView::Detail => draw_detail_view(f, app),
            },
            LoadingState::Loading => { /* Should not happen */ }
            LoadingState::Error(ref msg) => {
                draw_status_screen(f, &format!("Failed to load data. {}", msg))
            }
        })?;

        // Handle Events (User Input)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.loading_state {
                    LoadingState::Loading | LoadingState::Error(_) => match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => return Ok(()),
                        _ => {}
                    },
                    _ => {
                        if app.is_filtering {
                            match key.code {
                                KeyCode::Enter | KeyCode::Esc => {
                                    app.is_filtering = false;
                                    if key.code == KeyCode::Esc {
                                        app.filter_query.clear();
                                        app.clamp_selection();
                                    }
                                }
                                KeyCode::Backspace => {
                                    app.filter_query.pop();
                                    app.clamp_selection();
                                }
                                KeyCode::Char(c) => {
                                    if c != '/' {
                                        app.filter_query.push(c);
                                        app.clamp_selection();
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
                                            app.is_list_details_hover_visible = false;
                                            app.jump_to_start();
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.jump_to_end,
                                        ) {
                                            app.is_list_details_hover_visible = false;
                                            app.jump_to_end();
                                        } else if key_matches_sequence(c, last_key, &app.keys.quit)
                                        {
                                            return Ok(());
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.search,
                                        ) {
                                            app.is_list_details_hover_visible = false;
                                            app.is_filtering = true;
                                            app.filter_query.clear();
                                            app.clamp_selection();
                                        } else if key_matches_sequence(c, last_key, &app.keys.next)
                                        {
                                            app.is_list_details_hover_visible = false;
                                            app.navigate_list(1);
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.previous,
                                        ) {
                                            app.is_list_details_hover_visible = false;
                                            app.navigate_list(-1);
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.next_board,
                                        ) {
                                            app.is_list_details_hover_visible = false;
                                            app.next_board();
                                            return Ok(());
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.previous_board,
                                        ) {
                                            app.is_list_details_hover_visible = false;
                                            app.previous_board();
                                            return Ok(());
                                        } else if key_matches_sequence(c, last_key, &app.keys.hover)
                                        {
                                            app.is_list_details_hover_visible = true;
                                        } else if key_matches_sequence(c, last_key, &app.keys.open)
                                        {
                                            app.open_item();
                                        } else if key_matches_sequence(
                                            c,
                                            last_key,
                                            &app.keys.assigned_to_me_filter,
                                        ) {
                                            app.toggle_assigned_to_me_filter()
                                        }
                                        app.last_key_press = Some(key.code);
                                    } else {
                                        match key.code {
                                            KeyCode::Enter => {
                                                app.is_list_details_hover_visible = false;
                                                if app.list_state.selected().is_some() {
                                                    app.view = AppView::Detail;
                                                }
                                            }
                                            KeyCode::Esc => {
                                                if app.assigned_to_me_filter_on {
                                                    app.toggle_assigned_to_me_filter()
                                                }
                                                app.is_list_details_hover_visible = false;
                                                if !app.filter_query.is_empty() {
                                                    app.filter_query.clear();
                                                    app.clamp_selection();
                                                }
                                            }
                                            KeyCode::Up => {
                                                app.is_list_details_hover_visible = false;
                                                app.navigate_list(-1);
                                            }
                                            KeyCode::Down => {
                                                app.is_list_details_hover_visible = false;
                                                app.navigate_list(1);
                                            }
                                            _ => {}
                                        }
                                        app.last_key_press = None;
                                    }
                                }
                                AppView::Detail => {
                                    if let Some(c) = current_char {
                                        let last_key = app.last_key_press;

                                        if key_matches_sequence(c, last_key, &app.keys.quit) {
                                            app.view = AppView::List
                                        }
                                    } else {
                                        match key.code {
                                            KeyCode::Esc => app.view = AppView::List,
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
