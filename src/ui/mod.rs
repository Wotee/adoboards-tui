//! UI module
//!
//! This module contains all UI-related functionality:
//! - Rendering and drawing functions
//! - UI state management
//! - UI controller for state coordination

pub mod controller;

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
    let mut popup_width = 45;

    let selected_y_on_screen = list_area.y + 1 + relative_y;

    let indent = 2;
    let mut x = list_area.x.saturating_add(indent);
    let mut y = selected_y_on_screen + 1;

    let list_max_width = list_area.width.saturating_sub(2);
    popup_width = popup_width
        .min(list_max_width)
        .min(frame_area.width.saturating_sub(2));

    if y + popup_height > frame_area.height {
        y = selected_y_on_screen.saturating_sub(popup_height);
    }

    y = y.max(frame_area.y);

    let list_right_bound = list_area
        .x
        .saturating_add(list_area.width)
        .saturating_sub(popup_width + 1);
    let frame_right_bound = frame_area
        .width
        .saturating_sub(popup_width + 1)
        .max(frame_area.x + 1);
    x = x.min(list_right_bound).min(frame_right_bound);
    x = x.max(list_area.x + 1).max(frame_area.x + 1);

    Some(Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    })
}

fn calculate_detail_picker_rect(
    frame_area: Rect,
    field_area: Rect,
    content_lines: u16,
) -> Option<Rect> {
    if frame_area.width < 3 || frame_area.height < 3 {
        return None;
    }
    let popup_width = 45.min(frame_area.width.saturating_sub(2));
    let desired_height = content_lines.saturating_add(2);
    let popup_height = desired_height
        .max(3)
        .min(frame_area.height.saturating_sub(1));

    let mut x = field_area.x.saturating_add(1);
    let mut y = field_area.y.saturating_add(field_area.height);

    if y + popup_height > frame_area.height {
        y = field_area.y.saturating_sub(popup_height);
    }
    y = y.max(frame_area.y);

    if x + popup_width > frame_area.width {
        x = frame_area
            .width
            .saturating_sub(popup_width + 1)
            .max(frame_area.x + 1);
    }
    x = x.max(frame_area.x + 1);

    Some(Rect {
        x,
        y,
        width: popup_width,
        height: popup_height,
    })
}

fn draw_picker_popup(
    f: &mut ratatui::Frame,
    picker: &crate::app::PickerState,
    title: &str,
    rect: Rect,
) {
    let mut content_lines: Vec<Line> = Vec::new();

    if picker.options.is_empty() {
        content_lines.push(Line::from("No options"));
    } else {
        for (idx, t) in picker.options.iter().enumerate() {
            let is_selected = Some(idx) == picker.selected;
            let is_active = picker.active.contains(t);
            // Show [x] for the currently selected/highlighted item
            let indicator = if is_selected { "[x]" } else { "[ ]" };
            let line = if is_selected {
                Line::from(Span::styled(
                    format!("{} {}", indicator, t),
                    Style::default()
                        .fg(Color::Cyan)
                        .add_modifier(Modifier::BOLD),
                ))
            } else if is_active {
                // For multi-select: show checked state with different styling
                Line::from(Span::styled(
                    format!("[x] {}", t),
                    Style::default().fg(Color::Green),
                ))
            } else {
                Line::from(format!("{} {}", indicator, t))
            };
            content_lines.push(line);
        }
    }

    f.render_widget(Clear, rect);

    let popup_block = Block::default()
        .borders(Borders::ALL)
        .title(title)
        .border_style(Style::default().fg(Color::LightBlue));
    f.render_widget(Paragraph::new(content_lines).block(popup_block), rect);
}

fn draw_type_filter_popup(f: &mut ratatui::Frame, app: &mut App, list_area: Rect) {
    if !app.list_view_state.type_picker.is_open {
        return;
    }

    let content_height = app.list_view_state.type_picker.options.len().max(1) as u16;

    if let Some(popup_rect) = calculate_type_filter_rect(f.area(), app, list_area, content_height) {
        draw_picker_popup(
            f,
            &app.list_view_state.type_picker,
            "Type Filter",
            popup_rect,
        );
    }
}

fn draw_detail_picker_popup(
    f: &mut ratatui::Frame,
    picker: &crate::app::PickerState,
    field_area: Rect,
) {
    let content_height = picker.options.len().max(1) as u16;
    if let Some(popup_rect) = calculate_detail_picker_rect(f.area(), field_area, content_height) {
        draw_picker_popup(f, picker, "Select Value", popup_rect);
    }
}

pub fn draw_help_popup(f: &mut ratatui::Frame, app: &App) {
    if !app.showing_help {
        return;
    }

    let area = f.area();
    let width = (area.width as f32 * 0.8).round() as u16;
    let height = (area.height as f32 * 0.8).round() as u16;
    let x = area.x + (area.width.saturating_sub(width)) / 2;
    let y = area.y + (area.height.saturating_sub(height)) / 2;

    let popup_rect = Rect {
        x,
        y,
        width,
        height,
    };

    let mut lines: Vec<Line> = Vec::new();

    let keys = &app.keys;
    let key = |k: &str| {
        Span::styled(
            k.to_string(),
            Style::default()
                .fg(Color::Cyan)
                .add_modifier(Modifier::BOLD),
        )
    };

    lines.push(Line::from("List"));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.quit),
        Span::raw(" quit"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.next),
        Span::raw(" next / "),
        key(&keys.previous),
        Span::raw(" previous"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.jump_to_top),
        Span::raw(" top / "),
        key(&keys.jump_to_end),
        Span::raw(" end"),
    ]));
    lines.push(Line::from(vec![Span::raw("  Enter open item")]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.search),
        Span::raw(" search"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.work_item_type_filter),
        Span::raw(" type filter, "),
        key(&keys.assigned_to_me_filter),
        Span::raw(" assigned-to-me"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.next_board),
        Span::raw(" next board / "),
        key(&keys.previous_board),
        Span::raw(" prev board"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.refresh),
        Span::raw(" refresh / "),
        key(&keys.full_refresh),
        Span::raw(" full refresh"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.edit_config),
        Span::raw(" edit config"),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from("Detail"));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.open),
        Span::raw(" open in browser"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key(&keys.edit_item),
        Span::raw(" edit item"),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from("Edit Mode"));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key("Enter"),
        Span::raw(" save"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key("Tab"),
        Span::raw(" / "),
        key("Shift-Tab"),
        Span::raw(" move field"),
    ]));
    lines.push(Line::from(vec![
        Span::raw("  "),
        key("Esc"),
        Span::raw(" cancel edit"),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from("Type Filter"));
    lines.push(Line::from("  ↑/↓ move, Space/Enter toggle"));
    lines.push(Line::from("  c clear filters, Esc close"));

    lines.push(Line::from(""));
    lines.push(Line::from(vec![
        Span::raw("Help: "),
        key(&keys.help),
        Span::raw(" (toggle)"),
    ]));

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::LightBlue))
        // .style(Style::default().bg(Color::Black))
        .title("Hotkeys");
    let paragraph = Paragraph::new(lines)
        // .style(Style::default().bg(Color::Black))
        .wrap(Wrap { trim: false })
        .block(block);
    f.render_widget(Clear, popup_rect);
    f.render_widget(paragraph, popup_rect);
}

pub fn draw_list_view(f: &mut ratatui::Frame, app: &mut App, area: Rect) {
    let constraints = if app.list_view_state.is_filtering {
        [Constraint::Min(0), Constraint::Length(3)]
    } else {
        [Constraint::Min(0), Constraint::Length(0)]
    };

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints.iter().copied())
        .split(area);

    let items_to_display = app.get_filtered_items();

    let list_items: Vec<ListItem> = if items_to_display.is_empty() {
        vec![
            ListItem::new(Line::from(
                "No items match filters — press c in type filter to clear",
            ))
            .style(Style::default()),
        ]
    } else {
        items_to_display
            .iter()
            .map(|item| {
                let content = Line::from(format!("{}", item.title));
                ListItem::new(content).style(Style::default())
            })
            .collect()
    };

    let type_filter_label = if app.list_view_state.type_picker.active.is_empty() {
        "".to_string()
    } else {
        let joined = app
            .list_view_state
            .type_picker
            .active
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
                .add_modifier(Modifier::BOLD),
        );

    let list_area = chunks[0];
    f.render_widget(Clear, list_area);
    f.render_stateful_widget(list, list_area, &mut app.list_view_state.list_state);

    draw_type_filter_popup(f, app, list_area);

    if app.list_view_state.is_filtering {
        let filter_block = Block::default()
            .borders(Borders::ALL)
            .border_style(Style::default().fg(Color::LightBlue))
            .title("Filter Mode");

        let filter_text = Line::from(format!("/{}", app.list_view_state.filter_query));
        let filter_paragraph = Paragraph::new(filter_text).block(filter_block);
        f.render_widget(Clear, chunks[1]);
        f.render_widget(filter_paragraph, chunks[1]);

        let x = chunks[1].x + 2 + app.list_view_state.filter_query.len() as u16;
        let y = chunks[1].y + 1;
        f.set_cursor_position(ratatui::layout::Position::new(x, y));
    }
}

pub fn draw_detail_view(f: &mut ratatui::Frame, app: &App, area: Rect) {
    f.render_widget(Clear, area);
    let filtered_items = app.get_filtered_items();
    let selected_index = app.list_view_state.list_state.selected().unwrap_or(0);
    let item = match filtered_items.get(selected_index) {
        Some(item) => item,
        None => {
            let block = Block::default()
                .borders(Borders::ALL)
                .border_style(Color::LightBlue)
                .title("Details");
            let empty = Paragraph::new(Line::from("No item selected"))
                .style(Style::default().fg(Color::DarkGray))
                .block(block)
                .wrap(Wrap { trim: true });
            f.render_widget(empty, area);
            return;
        }
    };

    let edit_state = app.detail_view_state.edit_state.as_ref();
    let is_editing = edit_state.map(|s| s.is_editing).unwrap_or(false);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(
            [
                Constraint::Length(3),
                Constraint::Length(3),
                Constraint::Min(0),
            ]
            .as_ref(),
        )
        .split(area);

    let mut fields_to_render = if let Some(state) = edit_state {
        state.visible_fields.clone()
    } else {
        let source = app.current_source();
        let cache_key = (
            source.organization.clone(),
            source.project.clone(),
            item.work_item_type.clone(),
        );

        app.layout_cache()
            .get(&cache_key)
            .map(|controls| {
                controls
                    .iter()
                    .filter_map(|(id, label)| {
                        item.fields.get(id).map(|value| {
                            let allowed_values = app
                                .field_meta_cache()
                                .get(&item.work_item_type)
                                .and_then(|fields| {
                                    fields
                                        .iter()
                                        .find(|f| f.reference_name == *id)
                                        .map(|f| f.allowed_values.clone())
                                });
                            crate::ui_state::VisibleField::with_value(
                                label.clone(),
                                id.clone(),
                                value.clone(),
                                allowed_values,
                            )
                        })
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default()
    };

    // Add State and Assigned To fields at the beginning (only when not editing - they're already in edit_state when editing)
    if !is_editing {
        let state_allowed_values =
            app.field_meta_cache()
                .get(&item.work_item_type)
                .and_then(|fields| {
                    fields
                        .iter()
                        .find(|f| f.reference_name == "System.State")
                        .map(|f| f.allowed_values.clone())
                });
        let assigned_to_allowed_values =
            app.field_meta_cache()
                .get(&item.work_item_type)
                .and_then(|fields| {
                    fields
                        .iter()
                        .find(|f| f.reference_name == "System.AssignedTo")
                        .map(|f| f.allowed_values.clone())
                });

        let state_field = crate::ui_state::VisibleField::with_value(
            "State".to_string(),
            "System.State".to_string(),
            item.state.clone(),
            state_allowed_values,
        );
        let assigned_to_field = crate::ui_state::VisibleField::with_value(
            "Assigned To".to_string(),
            "System.AssignedTo".to_string(),
            item.assigned_to.clone(),
            assigned_to_allowed_values,
        );

        // Insert at the beginning of the fields list
        fields_to_render.insert(0, assigned_to_field);
        fields_to_render.insert(0, state_field);
    }

    let (title_value, active_field) = if let Some(state) = edit_state {
        (state.title.clone(), state.active_field)
    } else {
        (item.title.clone(), DetailField::Title)
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

    // Collect picker popup render info to render after all fields
    let mut picker_popups: Vec<(usize, Rect)> = Vec::new();

    // Render State and Assigned To fields side-by-side in chunks[1]
    if fields_to_render.len() >= 2 {
        let status_row = Layout::default()
            .direction(Direction::Horizontal)
            .constraints([Constraint::Percentage(50), Constraint::Percentage(50)].as_ref())
            .split(chunks[1]);

        // Render State field (index 0)
        let is_state_active =
            matches!(active_field, DetailField::Dynamic(active_idx) if active_idx == 0);
        let state_block = Block::default()
            .title(fields_to_render[0].label.as_str())
            .borders(Borders::ALL)
            .border_type(if is_editing && is_state_active {
                ratatui::widgets::BorderType::Thick
            } else {
                ratatui::widgets::BorderType::Plain
            })
            .border_style(Style::default().fg(if is_editing && is_state_active {
                Color::Cyan
            } else {
                Color::LightBlue
            }));
        let state_lines = vec![Line::from(Span::raw(fields_to_render[0].value.clone()))];
        let state_paragraph = Paragraph::new(state_lines).block(state_block);
        f.render_widget(state_paragraph, status_row[0]);

        // Render Assigned To field (index 1)
        let is_assigned_active =
            matches!(active_field, DetailField::Dynamic(active_idx) if active_idx == 1);
        let assigned_block = Block::default()
            .title(fields_to_render[1].label.as_str())
            .borders(Borders::ALL)
            .border_type(if is_editing && is_assigned_active {
                ratatui::widgets::BorderType::Thick
            } else {
                ratatui::widgets::BorderType::Plain
            })
            .border_style(Style::default().fg(if is_editing && is_assigned_active {
                Color::Cyan
            } else {
                Color::LightBlue
            }));
        let assigned_lines = vec![Line::from(Span::raw(fields_to_render[1].value.clone()))];
        let assigned_paragraph = Paragraph::new(assigned_lines).block(assigned_block);
        f.render_widget(assigned_paragraph, status_row[1]);

        // Collect picker popup info for State and Assigned To if active
        if is_editing && is_state_active {
            picker_popups.push((0, status_row[0]));
        }
        if is_editing && is_assigned_active {
            picker_popups.push((1, status_row[1]));
        }
    }

    // Handle remaining fields (index 2 and beyond) in chunks[2]
    let remaining_fields: Vec<_> = fields_to_render.iter_mut().skip(2).collect();
    if remaining_fields.is_empty() {
        fields_to_render.push(crate::ui_state::VisibleField::with_value(
            "No layout fields".to_string(),
            "".to_string(),
            "No fields for this layout".to_string(),
            None,
        ));
    }
    let constraints: Vec<Constraint> = fields_to_render
        .iter()
        .skip(2)
        .map(|field| {
            if field
                .picker
                .as_ref()
                .is_some_and(|picker| !picker.options.is_empty())
            {
                Constraint::Length(3)
            } else {
                Constraint::Min(3)
            }
        })
        .collect();
    let field_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints(constraints)
        .split(chunks[2]);

    for (idx, (field, area)) in fields_to_render
        .iter_mut()
        .skip(2)
        .zip(field_chunks.iter())
        .enumerate()
    {
        let actual_idx = idx + 2; // Account for the first two fields already rendered
        let is_active =
            matches!(active_field, DetailField::Dynamic(active_idx) if active_idx == actual_idx);
        let block = Block::default()
            .title(field.label.as_str())
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

        let lines = vec![Line::from(Span::raw(field.value.clone()))];
        let wrap = if field
            .picker
            .as_ref()
            .is_some_and(|picker| !picker.options.is_empty())
        {
            Wrap { trim: true }
        } else {
            Wrap { trim: false }
        };

        let paragraph = Paragraph::new(lines).wrap(wrap).block(block);
        f.render_widget(paragraph, *area);

        if is_editing && is_active {
            picker_popups.push((actual_idx, *area));
        }
    }

    // Render picker popups after all fields have been rendered
    for (field_idx, area) in picker_popups {
        if let Some(picker) = fields_to_render[field_idx].picker.as_ref() {
            draw_detail_picker_popup(f, picker, area);
        }
    }

    let status_line = match &app.detail_view_state.save_status {
        crate::ui_state::SaveStatus::Idle => None,
        crate::ui_state::SaveStatus::Saving => Some("Saving...".to_string()),
        crate::ui_state::SaveStatus::Failed(msg) => Some(format!("Save failed: {}", msg)),
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
            y: chunks[2].y.saturating_sub(3).max(chunks[0].y + 6),
            width: chunks[2].width,
            height: 3,
        };
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

    f.render_widget(Clear, chunks[1]);
    f.render_widget(paragraph, chunks[1]);
}
