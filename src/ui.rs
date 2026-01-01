use ratatui::{
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Modifier, Style},
    text::{Line, Span},
    widgets::{Block, Borders, Clear, List, ListItem, Paragraph, Wrap},
};

use crate::app::App;
use crate::models::DetailField;

fn calculate_popup_rect(frame_area: Rect, app: &App, list_area: Rect) -> Option<Rect> {
    let selected_index = app.list_view_state.list_state.selected()?;
    let offset = app.list_view_state.list_state.offset();

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

fn calculate_type_filter_rect(
    frame_area: Rect,
    app: &App,
    list_area: Rect,
    content_lines: u16,
) -> Option<Rect> {
    let selected_index = app.list_view_state.list_state.selected()?;
    let offset = app.list_view_state.list_state.offset();
    let relative_y = (selected_index.saturating_sub(offset)) as u16;

    let desired_height = content_lines.saturating_add(2);
    let popup_height = desired_height
        .max(3)
        .min(frame_area.height.saturating_sub(1));
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
    if app.list_view_state.is_list_details_hover_visible {
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

fn draw_type_filter_popup(f: &mut ratatui::Frame, app: &mut App, list_area: Rect) {
    if !app.list_view_state.is_type_filter_open {
        return;
    }

    let mut content_lines: Vec<Line> = Vec::new();

    if app.list_view_state.available_types.is_empty() {
        content_lines.push(Line::from("No types"));
    } else {
        for (idx, t) in app.list_view_state.available_types.iter().enumerate() {
            let is_selected = Some(idx) == app.list_view_state.type_filter_selection;
            let is_active = app.list_view_state.active_type_filters.contains(t);
            let indicator = if is_active { "[x]" } else { "[ ]" };
            let line = if is_selected {
                Line::from(Span::styled(
                    format!("{} {}", indicator, t),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else {
                Line::from(format!("{} {}", indicator, t))
            };
            content_lines.push(line);
        }
    }

    let content_height = content_lines.len() as u16;

    if let Some(popup_rect) = calculate_type_filter_rect(f.area(), app, list_area, content_height) {
        f.render_widget(Clear, popup_rect);

        let popup_block = Block::default()
            .borders(Borders::ALL)
            .title("Type Filter")
            .border_style(Style::default().fg(Color::LightBlue));
        f.render_widget(Paragraph::new(content_lines).block(popup_block), popup_rect);
    }
}

pub fn draw_list_view(f: &mut ratatui::Frame, app: &mut App) {
    let constraints = if app.list_view_state.is_filtering {
        [Constraint::Min(0), Constraint::Length(3)]
    } else {
        [Constraint::Min(0), Constraint::Length(0)]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.iter().copied())
        .split(f.area());

    let items_to_display = app.get_filtered_items();

    let list_items: Vec<ListItem> = if items_to_display.is_empty() {
        vec![
            ListItem::new(Line::from(
                "No items match filters — press c in type filter to clear",
            ))
            .style(Style::default().fg(Color::DarkGray)),
        ]
    } else {
        items_to_display
            .iter()
            .map(|item| {
                let content = Line::from(format!("{}: {}", item.id, item.title));
                ListItem::new(content).style(Style::default().fg(Color::White))
            })
            .collect()
    };

    let type_filter_label = if app.list_view_state.active_type_filters.is_empty() {
        "".to_string()
    } else {
        let joined = app
            .list_view_state
            .active_type_filters
            .iter()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        format!(" | Types: {}", joined)
    };

    let base_title = app.current_title();
    let board_title: String = if app.list_view_state.assigned_to_me_filter_on {
        format!(
            "{}, Assigned to {}{}",
            base_title,
            if app.me.is_empty() {
                "<name not configured>".to_string()
            } else {
                app.me.to_string()
            },
            type_filter_label,
        )
    } else {
        format!("{} {}", base_title, type_filter_label)
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
    f.render_stateful_widget(list, chunks[0], &mut app.list_view_state.list_state);

    draw_hover_popup(f, app, list_area);
    draw_type_filter_popup(f, app, list_area);

    if app.list_view_state.is_filtering {
        let filter_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::LightBlue))
            .title("Filter Mode");

        let filter_text = Line::from(format!("/{}", app.list_view_state.filter_query));
        let filter_paragraph = Paragraph::new(filter_text).block(filter_block);
        f.render_widget(filter_paragraph, chunks[1]);

        let x = chunks[1].x + 2 + app.list_view_state.filter_query.len() as u16;
        let y = chunks[1].y + 1;
        f.set_cursor_position(ratatui::layout::Position::new(x, y));
    }
}

pub fn draw_detail_view(f: &mut ratatui::Frame, app: &App) {
    let filtered_items = app.get_filtered_items();
    let selected_index = app.list_view_state.list_state.selected().unwrap_or(0);
    let item = filtered_items.get(selected_index).expect(
        "Logic Error: Detail view opened but item selection is invalid for the filtered list.",
    );

    let edit_state = app.detail_view_state.edit_state.as_ref();
    let is_editing = edit_state.map(|s| s.is_editing).unwrap_or(false);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Length(3), Constraint::Min(0)].as_ref())
        .split(f.area());

    let (title_value, active_field, extra_fields) = if let Some(state) = edit_state {
        (
            state.title.clone(),
            state.active_field,
            state.visible_fields.clone(),
        )
    } else {
        (item.title.clone(), DetailField::Title, Vec::new())
    };

    let title_text = format!("{}: {}", item.id, title_value);
    let title_block = Block::default()
        .title(item.work_item_type.to_string())
        .borders(Borders::ALL)
        .border_type(if is_editing && active_field == DetailField::Title {
            ratatui::widgets::BorderType::Thick
        } else {
            ratatui::widgets::BorderType::Plain
        })
        .border_style(
            Style::default().fg(if is_editing && active_field == DetailField::Title {
                Color::Cyan
            } else {
                Color::LightBlue
            }),
        );
    let title_paragraph = Paragraph::new(title_text)
        .style(Style::default().add_modifier(Modifier::BOLD))
        .block(title_block);
    f.render_widget(title_paragraph, chunks[0]);

    let fields_to_render: Vec<(String, String)> = if extra_fields.is_empty() {
        vec![(
            "No layout fields".to_string(),
            "No fields for this layout".to_string(),
        )]
    } else {
        extra_fields
            .iter()
            .map(|(label, _, value)| (label.clone(), value.clone()))
            .collect()
    };

    let constraints: Vec<Constraint> = fields_to_render
        .iter()
        .map(|_| Constraint::Min(3))
        .collect();
    let field_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(chunks[1]);

    for (idx, ((key, value), area)) in fields_to_render.iter().zip(field_chunks.iter()).enumerate()
    {
        let is_active =
            matches!(active_field, DetailField::Dynamic(active_idx) if active_idx == idx);
        let block = Block::default()
            .title(key.as_str())
            .borders(Borders::ALL)
            .border_type(if is_editing && is_active {
                ratatui::widgets::BorderType::Thick
            } else {
                ratatui::widgets::BorderType::Plain
            })
            .border_style(Style::default().fg(if is_editing && is_active {
                Color::Cyan
            } else {
                Color::LightBlue
            }));

        let lines = vec![Line::from(Span::raw(value.clone()))];
        let paragraph = Paragraph::new(lines)
            .wrap(Wrap { trim: false })
            .block(block);
        f.render_widget(paragraph, *area);
    }

    let status_line = match &app.detail_view_state.save_status {
        crate::app::SaveStatus::Idle => None,
        crate::app::SaveStatus::Saving => Some("Saving...".to_string()),
        crate::app::SaveStatus::Failed(msg) => Some(format!("Save failed: {}", msg)),
    };

    if let Some(status) = status_line {
        let status_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::Yellow))
            .title("Status");
        let status_para = Paragraph::new(Line::from(status))
            .style(Style::default().fg(Color::Yellow))
            .block(status_block)
            .wrap(Wrap { trim: true });
        let status_area = Rect {
            x: chunks[0].x,
            y: chunks[1].y.saturating_sub(3).max(chunks[0].y + 3),
            width: chunks[1].width,
            height: 3,
        };
        f.render_widget(Clear, status_area);
        f.render_widget(status_para, status_area);
    }
}

pub fn draw_status_screen(f: &mut ratatui::Frame, message: &str) {
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
