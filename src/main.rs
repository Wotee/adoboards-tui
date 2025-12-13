use anyhow::Result;
use azure_devops_rust_api::Credential;
use azure_devops_rust_api::wit::ClientBuilder as WitClientBuilder;
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

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct CommonConfig {
    common: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct AppConfig {
    #[serde(default)]
    common: Option<CommonConfig>,
    #[serde(default)]
    boards: Vec<BoardConfig>,
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

    let mut app_items: Vec<WorkItem> = Vec::new();

    for item in full_items.value {
        let get_and_clean_field = |key: &str| -> String {
            item.fields
                .get(key)
                .and_then(|v| v.as_str())
                .map_or("N/A".to_string(), clean_ado_text)
        };
        let assigned_to_name: String = item
            .fields
            .get("System.AssignedTo")
            .and_then(|assigned_to| assigned_to.as_object())
            .and_then(|assigned_to| assigned_to.get("displayName"))
            .and_then(|display_name| display_name.as_str())
            .map(|s| s.to_string())
            .unwrap_or("Unassigned".to_string());

        app_items.push(WorkItem {
            id: item.id as u32,
            title: get_and_clean_field("System.Title"),
            work_item_type: get_and_clean_field("System.WorkItemType"),
            description: get_and_clean_field("System.Description"),
            acceptance_criteria: get_and_clean_field("Microsoft.VSTS.Common.AcceptanceCriteria"),
            assigned_to: assigned_to_name,
            state: get_and_clean_field("System.State"),
        });
    }

    return Ok(app_items);
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

/// The main application state struct.
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
}

impl App {
    fn new(boards: Vec<BoardConfig>) -> App {
        let mut list_state = ListState::default();
        if !boards.is_empty() {
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
            all_boards: boards,
            current_board_index: 0,
        }
    }

    fn current_board(&self) -> &BoardConfig {
        &self.all_boards[self.current_board_index]
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

    fn get_filtered_items(&self) -> Vec<&WorkItem> {
        if self.filter_query.is_empty() {
            return self.items.iter().collect();
        }

        let query = self.filter_query.to_lowercase();

        self.items
            .iter()
            .filter(|item| {
                let id_match = item.id.to_string().contains(&query);
                let title_match = item.title.to_lowercase().contains(&query);
                id_match || title_match
            })
            .collect()
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

    /// Moves the selection up in the list.
    fn previous(&mut self) {
        let items_len = self.get_filtered_items().len();
        if items_len == 0 {
            self.list_state.select(None);
            return;
        }

        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    items_len - 1
                } else {
                    i - 1
                }
            }
            None => items_len - 1,
        };
        self.list_state.select(Some(i));
    }

    /// Moves the selection down in the list.
    fn next(&mut self) {
        let items_len = self.get_filtered_items().len();
        if items_len == 0 {
            self.list_state.select(None);
            return;
        }
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= items_len - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }
}

fn load_config() -> Result<Vec<BoardConfig>> {
    let config_content = std::fs::read_to_string("config.toml")?;
    let app_config: AppConfig = toml::from_str(&config_content)?;
    if app_config.boards.is_empty() {
        return Err(anyhow::anyhow!(
            "Configuration file 'config.toml' is missing any board definitions."
        ));
    }
    Ok(app_config.boards)
}

// --- TUI Drawing Functions ---
fn calculate_popup_rect(frame_area: Rect, app: &App, list_area: Rect) -> Option<Rect> {
    let selected_index_in_filtered_list = app.list_state.selected()?;
    let content_height = list_area.height.saturating_sub(2); // subtract top/bottom border
    let selected_y_in_list =
        list_area.y + 1 + (selected_index_in_filtered_list % content_height as usize) as u16;

    let popup_height = 5; // Title, 2 content lines, 2 borders
    let popup_width = 45;

    let mut x = list_area.x + 20;
    let mut y = selected_y_in_list + 1; // Default: 1 line below the selected item

    if y + popup_height > frame_area.height {
        y = selected_y_in_list.saturating_sub(popup_height);
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

/// Renders the main List View (the board).
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

    let new_selection_index = {
        let items_to_display_count = app.get_filtered_items().len();
        let current_selected = app.list_state.selected();

        if current_selected.is_some()
            && current_selected.unwrap() >= items_to_display_count
            && items_to_display_count > 0
        {
            Some(items_to_display_count - 1)
        } else if items_to_display_count == 0 {
            None
        } else if current_selected.is_none() && items_to_display_count > 0 {
            Some(0)
        } else {
            current_selected
        }
    };

    if new_selection_index != app.list_state.selected() {
        app.list_state.select(new_selection_index);
    }
    let items_to_display = app.get_filtered_items();

    let list_items: Vec<ListItem> = items_to_display
        .iter()
        .map(|item| {
            let content = Line::from(format!("{}: {}", item.id, item.title));
            ListItem::new(content).style(Style::default().fg(Color::White))
        })
        .collect();

    let board_title = format!("{} Backlog", app.current_board().team);

    let list = List::new(list_items)
        .block(Block::default().borders(Borders::ALL).title(board_title))
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
            .border_style(Style::default().fg(Color::Yellow))
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

    let title_text = format!("{}: {} {}", item.work_item_type, item.id, item.title);
    let title_block = Block::default()
        .title("Work Item Details (Press ESC or 'q' to go back)")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Cyan));
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
    let block = Block::default().borders(Borders::ALL).title("Status");
    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            message,
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
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
                                    }
                                    app.list_state
                                        .select(app.get_filtered_items().first().map(|_| 0));
                                }
                                KeyCode::Backspace => {
                                    app.filter_query.pop();
                                    app.list_state
                                        .select(app.get_filtered_items().first().map(|_| 0));
                                }
                                KeyCode::Char(c) => {
                                    if c != '/' {
                                        app.filter_query.push(c);
                                        app.list_state
                                            .select(app.get_filtered_items().first().map(|_| 0));
                                    }
                                }
                                _ => {}
                            }
                        } else {
                            match app.view {
                                AppView::List => match key.code {
                                    KeyCode::Char('q') => return Ok(()),
                                    KeyCode::Char('/') => {
                                        app.is_list_details_hover_visible = false;
                                        app.is_filtering = true;
                                        app.filter_query.clear();
                                        app.list_state
                                            .select(app.get_filtered_items().first().map(|_| 0));
                                    }
                                    KeyCode::Up | KeyCode::Char('k') => {
                                        app.is_list_details_hover_visible = false;
                                        app.previous();
                                    }
                                    KeyCode::Down | KeyCode::Char('j') => {
                                        app.is_list_details_hover_visible = false;
                                        app.next();
                                    }
                                    KeyCode::Enter => {
                                        app.is_list_details_hover_visible = false;
                                        if app.list_state.selected().is_some() {
                                            app.view = AppView::Detail;
                                        }
                                    }
                                    KeyCode::Esc => {
                                        app.is_list_details_hover_visible = false;
                                        if !app.filter_query.is_empty() {
                                            app.filter_query.clear();
                                            app.list_state.select(
                                                app.get_filtered_items().first().map(|_| 0),
                                            );
                                        }
                                    }
                                    KeyCode::Char('n') => {
                                        app.is_list_details_hover_visible = false;
                                        app.next_board();
                                        return Ok(());
                                    }
                                    KeyCode::Char('p') => {
                                        app.is_list_details_hover_visible = false;
                                        app.previous_board();
                                        return Ok(());
                                    }
                                    KeyCode::Char('K') => {
                                        app.is_list_details_hover_visible = true;
                                    }
                                    _ => {}
                                },
                                AppView::Detail => match key.code {
                                    KeyCode::Char('q') | KeyCode::Esc => app.view = AppView::List,
                                    _ => {}
                                },
                            }
                        }
                    }
                }
            }
        }
    }
}
