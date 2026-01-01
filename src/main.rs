use std::collections::{BTreeMap, BTreeSet};
use std::io;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

mod app;
mod config;
mod models;
mod services;
mod ui;

use crate::app::{App, LoadingState, prefetch_layouts, run_app};
use crate::config::load_config_or_prompt;
use crate::services::{
    build_field_metadata_cache, fetch_process_template_type, fetch_process_work_item_types,
    fetch_project_id, get_backlog_ids, get_items, get_iteration_ids, resolve_iteration_id,
};
use crate::ui::draw_status_screen;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    let (cfg, config_ok) = load_config_or_prompt();
    let mut app = App::new(cfg);
    let mut res = Ok(());

    if config_ok {
        while !matches!(app.loading_state, LoadingState::Error(_)) {
            if matches!(app.loading_state, LoadingState::Loading) {
                let source = app.current_source().clone();
                let source_title = source.title.clone();
                terminal
                    .draw(|f| draw_status_screen(f, &format!("Loading {}...", source_title)))?;

                let fetch_result: Result<Vec<_>, anyhow::Error> = async {
                    let project_id =
                        fetch_project_id(&source.organization, &source.project).await?;
                    let process_template_type =
                        fetch_process_template_type(&source.organization, &project_id).await?;
                    app.set_process_template_type(process_template_type.clone());
                    let work_item_types =
                        fetch_process_work_item_types(&source.organization, &process_template_type)
                            .await?;
                    let work_item_types_map: BTreeMap<String, String> =
                        work_item_types.iter().cloned().collect();
                    app.set_work_item_types(work_item_types_map.clone());

                    #[allow(unreachable_code)]
                    let items_result = match source.kind {
                        crate::app::SourceKind::Backlog => {
                            let ids = get_backlog_ids(
                                &source.organization,
                                &source.project,
                                &source.team,
                            )
                            .await?;
                            let items =
                                get_items(&source.organization, &source.project, ids).await?;
                            Ok::<_, anyhow::Error>(items)
                        }
                        crate::app::SourceKind::Iteration(iteration) => {
                            let iteration_id = resolve_iteration_id(
                                &iteration.organization,
                                &iteration.project,
                                &iteration.team,
                                &iteration.iteration,
                            )
                            .await?;
                            let ids = get_iteration_ids(
                                &iteration.organization,
                                &iteration.project,
                                &iteration.team,
                                &iteration_id,
                            )
                            .await?;
                            let items =
                                get_items(&iteration.organization, &iteration.project, ids).await?;
                            Ok::<_, anyhow::Error>(items)
                        }
                    }?;

                    let used_types: BTreeSet<String> = items_result
                        .iter()
                        .map(|item| item.work_item_type.clone())
                        .collect();

                    // work_item_types: display name -> reference name
                    // layouts require reference names; field metadata requires display names
                    let mut layout_reference_names: Vec<String> = Vec::new();
                    let mut metadata_display_names: Vec<String> = Vec::new();
                    for (display, reference) in work_item_types {
                        if used_types.contains(&display) {
                            layout_reference_names.push(reference);
                            metadata_display_names.push(display);
                        }
                    }

                    let organization = source.organization.clone();
                    let fields_organization = organization.clone();
                    let process_id = process_template_type.clone();
                    let fields_project = source.project.clone();
                    let layout_handle = tokio::spawn(async move {
                        prefetch_layouts(&organization, &process_id, layout_reference_names).await
                    });
                    let fields_handle = tokio::spawn(async move {
                        build_field_metadata_cache(
                            &fields_organization,
                            &fields_project,
                            metadata_display_names,
                        )
                        .await
                    });

                    if let Ok(prefetched) = layout_handle.await {
                        app.layout_cache = prefetched;
                    }
                    if let Ok(meta) = fields_handle.await {
                        app.field_meta_cache = meta;
                    }

                    Ok(items_result)
                }
                .await;

                match fetch_result {
                    Ok(items) => app.load_data(items),
                    Err(e) => {
                        let error_msg = format!("Failed to fetch data: {e:?}");
                        eprintln!("\n--- FATAL FETCH ERROR ---\n{}", error_msg);
                        app.loading_state = LoadingState::Error(error_msg);
                    }
                }
                continue;
            }

            res = run_app(&mut terminal, &mut app).await;
            if res.is_err() {
                break;
            }

            if matches!(app.loading_state, LoadingState::Loading) {
                continue;
            }
            break;
        }
    }

    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err);
    }

    Ok(())
}
