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
    layout::{Constraint, Direction, Layout},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
};
use regex::Regex;
use std::{error::Error, io, time::Duration};

// --- Data Model Structs ---

/// Represents a simple work item returned by the API.
#[derive(Clone, Debug)]
pub struct WorkItem {
    id: u32,
    title: String,
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

        app_items.push(WorkItem {
            id: item.id as u32,
            title: get_and_clean_field("System.Title"),
            work_item_type: get_and_clean_field("System.WorkItemType"),
            description: get_and_clean_field("System.Description"),
            acceptance_criteria: get_and_clean_field("Microsoft.VSTS.Common.AcceptanceCriteria"),
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
    /// The current view (List or Detail).
    view: AppView,
    /// The list of work items fetched from Azure DevOps.
    items: Vec<WorkItem>,
    /// State management for the List widget (which item is selected).
    list_state: ListState,
    /// Tracks the current data fetching state.
    loading_state: LoadingState,
    board_title: String,
}

impl App {
    fn new(board_title: String) -> App {
        App {
            view: AppView::List,
            items: Vec::new(),
            list_state: ListState::default(),
            loading_state: LoadingState::Loading,
            board_title: board_title,
        }
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
        let i = match self.list_state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.list_state.select(Some(i));
    }

    /// Moves the selection down in the list.
    fn next(&mut self) {
        let i = match self.list_state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
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

// --- TUI Drawing Functions ---

/// Renders the main List View (the board).
fn draw_list_view(f: &mut ratatui::Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .margin(1)
        .constraints([Constraint::Percentage(100)].as_ref())
        .split(f.area());

    let items: Vec<ListItem> = app
        .items
        .iter()
        .map(|item| {
            let content = Line::from(format!("{}: {}", item.id, item.title));
            ListItem::new(content).style(Style::default().fg(Color::White))
        })
        .collect();

    let list = List::new(items)
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title(app.board_title.clone()),
        )
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .fg(Color::White)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, chunks[0], &mut app.list_state);
}

/// Renders the Detail View for the selected work item.
fn draw_detail_view(f: &mut ratatui::Frame, app: &App) {
    // Get the selected work item
    let selected_item_index = app.list_state.selected().unwrap_or(0);
    let item = &app.items[selected_item_index];

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
    let org = "waypoint-azure";
    let project = "HealthHub";
    let team = "HealthHub Team";
    let board_title = format!("{} Backlog", team);

    // 1. Terminal setup
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let mut app = App::new(board_title);
    let _ = terminal.draw(|f| draw_status_screen(f, "Connecting to Azure DevOps..."));
    let fetch_result = get_backlog(&org, &project, &team).await;

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

    // 5. Run the TUI loop
    let res = run_app(&mut terminal, &mut app);

    // 3. Terminal cleanup
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
    loop {
        // Draw the UI based on the current view
        terminal.draw(|f| match app.loading_state {
            LoadingState::Loaded => match app.view {
                AppView::List => draw_list_view(f, app),
                AppView::Detail => draw_detail_view(f, app),
            },
            LoadingState::Loading => draw_status_screen(f, "Loading work items..."),
            LoadingState::Error(ref msg) => {
                draw_status_screen(f, &format!("Failed to load data. {}", msg))
            }
        })?;

        // Handle Events (User Input)
        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                match app.loading_state {
                    LoadingState::Loading | LoadingState::Error(_) => match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        _ => {}
                    },
                    _ => match app.view {
                        AppView::List => match key.code {
                            KeyCode::Char('q') => return Ok(()),
                            KeyCode::Up | KeyCode::Char('k') => app.previous(),
                            KeyCode::Down | KeyCode::Char('j') => app.next(),
                            KeyCode::Enter => {
                                if app.list_state.selected().is_some() {
                                    app.view = AppView::Detail;
                                }
                            }
                            _ => {}
                        },
                        AppView::Detail => match key.code {
                            KeyCode::Char('q') | KeyCode::Esc => app.view = AppView::List,
                            _ => {}
                        },
                    },
                }
            }
        }
    }
}
