use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::io;
use std::time::Duration;

use crossterm::{
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};

mod app;
mod cache;
mod config;
mod models;
mod services;
mod ui;

use crate::app::{App, LoadingState, RefreshPolicy, prefetch_layouts, run_app};
use crate::cache::{
    LayoutCacheKey, WorkItemsCacheKey, read_field_meta_cache, read_layout_cache,
    read_work_items_cache, write_work_items_cache,
};
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
                    let refresh_policy = app.refresh_policy.clone();
                    let max_age = Duration::from_secs(3600);

                    // Reset caches if explicitly refreshing
                    if matches!(refresh_policy, RefreshPolicy::Full) {
                        app.clear_layout_cache();
                        app.field_meta_cache.clear();
                    }

                    // 1) Work items: try cache first
                    let items_result = match source.kind {
                        crate::app::SourceKind::Backlog => {
                            let cache_key = WorkItemsCacheKey::Backlog {
                                organization: source.organization.clone(),
                                project: source.project.clone(),
                                team: source.team.clone(),
                            };
                            let cached = if matches!(refresh_policy, RefreshPolicy::Normal) {
                                read_work_items_cache(&cache_key, max_age)
                            } else {
                                None
                            };
                            if let Some(items) = cached {
                                Ok::<_, anyhow::Error>(items)
                            } else {
                                let ids = get_backlog_ids(
                                    &source.organization,
                                    &source.project,
                                    &source.team,
                                )
                                .await?;
                                 let items = get_items(&source.organization, &source.project, ids).await?;
                                 let _ = write_work_items_cache(&cache_key, &items);

                                Ok::<_, anyhow::Error>(items)
                            }
                        }
                        crate::app::SourceKind::Iteration(iteration) => {
                            let cache_key = WorkItemsCacheKey::Iteration {
                                organization: iteration.organization.clone(),
                                project: iteration.project.clone(),
                                team: iteration.team.clone(),
                                iteration: iteration.iteration.clone(),
                            };
                            let cached = if matches!(refresh_policy, RefreshPolicy::Normal) {
                                read_work_items_cache(&cache_key, max_age)
                            } else {
                                None
                            };
                             if let Some(items) = cached {
                                 Ok::<_, anyhow::Error>(items)
                             } else {

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
                                     get_items(&iteration.organization, &iteration.project, ids)
                                         .await?;
                                 let _ = write_work_items_cache(&cache_key, &items);

                                Ok::<_, anyhow::Error>(items)
                            }
                        }
                    }?;

                    let used_types: BTreeSet<String> = items_result
                        .iter()
                        .map(|item| item.work_item_type.clone())
                        .collect();

                    // 2) Determine which types need layout/field metadata
                    let metadata_display_names: Vec<String> =
                        used_types.iter().cloned().collect();
                    let mut missing_layout_displays: Vec<String> = Vec::new();

                    for display in &metadata_display_names {
                        let cache_key = (
                            source.organization.clone(),
                            source.project.clone(),
                            display.clone(),
                        );
                        let layout_key = LayoutCacheKey {
                            organization: source.organization.clone(),
                            project: source.project.clone(),
                            work_item_type: display.clone(),
                        };
                        let in_memory = app.layout_cache.get(&cache_key).is_some();
                        let on_disk = if matches!(refresh_policy, RefreshPolicy::Full) {
                            None
                        } else {
                            read_layout_cache(&layout_key)
                        };
                        if matches!(refresh_policy, RefreshPolicy::Full)
                            || (!in_memory && on_disk.is_none())
                        {
                            missing_layout_displays.push(display.clone());
                        } else if !in_memory {
                            if let Some(disk) = on_disk {
                                app.layout_cache.insert(cache_key, disk);
                            }
                        }
                    }

                    // 3) Determine if we need to fetch process/work item types
                    let mut process_id = app.process_template_type.clone();
                    let need_process_fetch =
                        matches!(refresh_policy, RefreshPolicy::Full)
                            || !missing_layout_displays.is_empty();

                    let mut layout_pairs: Vec<(String, String)> = Vec::new();

                    if need_process_fetch {
                        let project_id =
                            fetch_project_id(&source.organization, &source.project).await?;
                        let fetched_process_id =
                            fetch_process_template_type(&source.organization, &project_id).await?;
                        let fetched_work_item_types = fetch_process_work_item_types(
                            &source.organization,
                            &fetched_process_id,
                        )
                        .await?;

                        process_id = Some(fetched_process_id.clone());
                        let map: BTreeMap<String, String> =
                            fetched_work_item_types.iter().cloned().collect();
                        app.set_process_template_type(fetched_process_id);
                        app.set_work_item_types(map);

                        for (display, reference) in fetched_work_item_types {
                            if used_types.contains(&display)
                                && (matches!(refresh_policy, RefreshPolicy::Full)
                                    || missing_layout_displays.contains(&display))
                            {
                                layout_pairs.push((display.clone(), reference.clone()));
                            }
                        }
                         }


                    // 4) Kick off layout and field metadata fetches
                    let organization = source.organization.clone();
                    let project = source.project.clone();
                    let fields_organization = organization.clone();
                    let fields_project = project.clone();
                    let layout_refresh_policy = refresh_policy.clone();
                    let fields_refresh_policy = refresh_policy.clone();
                    let missing_field_meta = metadata_display_names
                        .iter()
                        .filter(|display_name| {
                            let cache_key = crate::cache::FieldMetaCacheKey {
                                organization: fields_organization.clone(),
                                project: fields_project.clone(),
                                work_item_type: (*display_name).clone(),
                            };
                            matches!(fields_refresh_policy, RefreshPolicy::Full)
                                || read_field_meta_cache(&cache_key).is_none()
                        })
                        .count();

                    let layout_handle = if layout_pairs.is_empty() {
                        tokio::spawn(async move { HashMap::new() })
                    } else {
                        let process_id_value = process_id.clone().unwrap_or_default();
                        tokio::spawn(async move {
                            prefetch_layouts(
                                &organization,
                                &project,
                                &process_id_value,
                                layout_pairs,
                                layout_refresh_policy,
                            )
                            .await
                        })
                    };
                    let fields_handle = tokio::spawn(async move {
                        // If everything is cached and refresh is normal, skip fetch
                        if missing_field_meta == 0
                            && matches!(fields_refresh_policy, RefreshPolicy::Normal)
                        {
                            let mut cache = std::collections::HashMap::new();
                            for display_name in metadata_display_names {
                                let cache_key = crate::cache::FieldMetaCacheKey {
                                    organization: fields_organization.clone(),
                                    project: fields_project.clone(),
                                    work_item_type: display_name.clone(),
                                };
                                if let Some(fields) = read_field_meta_cache(&cache_key) {
                                    cache.insert(display_name.clone(), fields);
                                }
                            }
                            return cache;
                        }

                        build_field_metadata_cache(
                            &fields_organization,
                            &fields_project,
                            metadata_display_names,
                            fields_refresh_policy,
                        )
                        .await
                    });

                    if let Ok(prefetched) = layout_handle.await {
                        if !prefetched.is_empty() {
                            app.layout_cache.extend(prefetched);
                        }
                    }
                    if let Ok(meta) = fields_handle.await {
                        app.field_meta_cache = meta;
                    }

                    if matches!(app.refresh_policy, RefreshPolicy::Full) {
                        app.refresh_policy = RefreshPolicy::Normal;
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
