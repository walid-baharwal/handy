#[cfg(feature = "desktop")]
mod detector;
mod model;
#[cfg(feature = "desktop")]
mod runtime;
#[cfg(feature = "desktop")]
mod store;

#[cfg(feature = "desktop")]
mod desktop {
    use crate::model::{Config, Group, HandyCommand, Project, TargetKind, TargetRef};
    use crate::runtime::{LogEntry, RuntimeEntry, RuntimeManager};
    use crate::store::ConfigStore;
    use serde::Serialize;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::{Arc, Mutex};
    use tauri::menu::{Menu, MenuItem};
    use tauri::tray::TrayIconBuilder;
    use tauri::{AppHandle, Manager, State, WindowEvent};

    struct AppState {
        config: Mutex<Config>,
        store: ConfigStore,
        runtime: Arc<RuntimeManager>,
        quitting: AtomicBool,
        startup_warning: Option<String>,
    }

    #[derive(Serialize)]
    struct Snapshot {
        config: Config,
        runtime: Vec<RuntimeEntry>,
        logs: Vec<LogEntry>,
        #[serde(skip_serializing_if = "Option::is_none")]
        startup_warning: Option<String>,
    }

    #[tauri::command]
    fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
        Snapshot {
            config: state.config.lock().unwrap().clone(),
            runtime: state.runtime.snapshot(),
            logs: state.runtime.log_snapshot(),
            startup_warning: state.startup_warning.clone(),
        }
    }

    #[tauri::command]
    fn detect_commands(
        base_dir: String,
    ) -> Result<Vec<crate::detector::CommandSuggestion>, String> {
        crate::detector::detect(&base_dir)
    }

    #[tauri::command]
    fn save_project(
        mut project: Project,
        commands: Vec<HandyCommand>,
        state: State<'_, AppState>,
    ) -> Result<Config, String> {
        if !std::path::Path::new(&project.base_dir).is_dir() {
            return Err("Choose an existing project folder".into());
        }
        if commands
            .iter()
            .any(|command| command.project_id != project.id)
        {
            return Err("All saved commands must belong to this project".into());
        }

        let mut config = state.config.lock().unwrap();
        if let Some(existing) = config.projects.get(&project.id) {
            let base_dir_changed = existing.base_dir != project.base_dir;
            for id in &existing.command_ids {
                let Some(old) = config.commands.get(id) else {
                    continue;
                };
                let replacement = commands.iter().find(|command| command.id == *id);
                let execution_changed = replacement.is_none_or(|new| {
                    old.command != new.command
                        || old.cwd != new.cwd
                        || old.stop_command != new.stop_command
                        || old.status_command != new.status_command
                });
                if state.runtime.can_stop(old) && (base_dir_changed || execution_changed) {
                    return Err(format!(
                        "Stop '{}' before changing or removing it",
                        old.name
                    ));
                }
            }
        }
        let mut next = config.clone();
        let old_commands = next
            .projects
            .get(&project.id)
            .map(|value| value.command_ids.clone())
            .unwrap_or_default();
        project.command_ids = commands.iter().map(|command| command.id.clone()).collect();
        for id in old_commands {
            next.commands.remove(&id);
        }
        for command in commands {
            next.commands.insert(command.id.clone(), command);
        }
        next.projects.insert(project.id.clone(), project);
        state.store.save(&next)?;
        *config = next.clone();
        Ok(next)
    }

    #[tauri::command]
    fn delete_project(id: String, state: State<'_, AppState>) -> Result<Config, String> {
        let mut config = state.config.lock().unwrap();
        if config
            .projects
            .get(&id)
            .into_iter()
            .flat_map(|project| &project.command_ids)
            .filter_map(|command_id| config.commands.get(command_id))
            .any(|command| state.runtime.can_stop(command))
        {
            return Err("Stop this project before deleting it".into());
        }
        let mut next = config.clone();
        let removed_commands = next
            .projects
            .remove(&id)
            .map(|project| project.command_ids)
            .unwrap_or_default();
        for command_id in &removed_commands {
            next.commands.remove(command_id);
        }
        for group in next.groups.values_mut() {
            group.targets.retain(|target| {
                !(target.kind == TargetKind::Project && target.id == id)
                    && !(target.kind == TargetKind::Command
                        && removed_commands.contains(&target.id))
            });
        }
        state.store.save(&next)?;
        *config = next.clone();
        Ok(next)
    }

    #[tauri::command]
    fn save_group(group: Group, state: State<'_, AppState>) -> Result<Config, String> {
        let mut config = state.config.lock().unwrap();
        if config
            .groups
            .get(&group.id)
            .is_some_and(|existing| existing.targets != group.targets)
            && target_is_stoppable(
                &config,
                &state.runtime,
                &TargetRef {
                    kind: TargetKind::Group,
                    id: group.id.clone(),
                },
            )?
        {
            return Err("Stop this group before changing its targets".into());
        }
        let mut next = config.clone();
        next.groups.insert(group.id.clone(), group);
        state.store.save(&next)?;
        *config = next.clone();
        Ok(next)
    }

    #[tauri::command]
    fn delete_group(id: String, state: State<'_, AppState>) -> Result<Config, String> {
        let mut config = state.config.lock().unwrap();
        if config.groups.contains_key(&id)
            && target_is_stoppable(
                &config,
                &state.runtime,
                &TargetRef {
                    kind: TargetKind::Group,
                    id: id.clone(),
                },
            )?
        {
            return Err("Stop this group before deleting it".into());
        }
        let mut next = config.clone();
        next.groups.remove(&id);
        for group in next.groups.values_mut() {
            group
                .targets
                .retain(|target| !(target.kind == TargetKind::Group && target.id == id));
        }
        state.store.save(&next)?;
        *config = next.clone();
        Ok(next)
    }

    #[tauri::command]
    async fn start_target(target: TargetRef, state: State<'_, AppState>) -> Result<(), String> {
        let (commands, projects, ids) = {
            let config = state.config.lock().unwrap();
            let ids = config.resolve(&target)?;
            let commands: Vec<_> = ids
                .iter()
                .filter_map(|id| config.commands.get(id).cloned())
                .collect();
            let projects = config.projects.clone();
            (commands, projects, ids)
        };
        state.runtime.activate(target, ids).await;
        for command in commands {
            let project = projects
                .get(&command.project_id)
                .cloned()
                .ok_or_else(|| "Command project is missing".to_string())?;
            state.runtime.start(command, project).await?;
        }
        Ok(())
    }

    #[tauri::command]
    async fn stop_target(target: TargetRef, state: State<'_, AppState>) -> Result<(), String> {
        let (commands, projects, ids) = {
            let config = state.config.lock().unwrap();
            let ids = config.resolve(&target)?;
            let commands = config.commands.clone();
            let projects = config.projects.clone();
            (commands, projects, ids)
        };
        let stop_ids = state.runtime.deactivate(&target, ids).await;
        for id in stop_ids {
            if let Some(command) = commands.get(&id) {
                if let Some(project) = projects.get(&command.project_id) {
                    state.runtime.stop(command, project).await;
                }
            }
        }
        Ok(())
    }

    #[tauri::command]
    fn clear_logs(state: State<'_, AppState>) -> u64 {
        state.runtime.clear_logs()
    }

    fn target_is_stoppable(
        config: &Config,
        runtime: &RuntimeManager,
        target: &TargetRef,
    ) -> Result<bool, String> {
        Ok(config
            .resolve(target)?
            .iter()
            .filter_map(|id| config.commands.get(id))
            .any(|command| runtime.can_stop(command)))
    }

    fn show_main(app: &AppHandle) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.unminimize();
            let _ = window.set_focus();
        }
    }

    fn begin_quit(app: &AppHandle) {
        let state = app.state::<AppState>();
        if state.quitting.swap(true, Ordering::SeqCst) {
            return;
        }
        let config = state.config.lock().unwrap().clone();
        let runtime = Arc::clone(&state.runtime);
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            runtime.shutdown(config).await;
            app.exit(0);
        });
    }

    #[cfg_attr(mobile, tauri::mobile_entry_point)]
    pub fn run() {
        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .setup(|app| {
                let store =
                    ConfigStore::new(app.path().app_data_dir()?).map_err(std::io::Error::other)?;
                let loaded = store.load();
                let runtime = Arc::new(RuntimeManager::new(app.handle().clone()));
                app.manage(AppState {
                    config: Mutex::new(loaded.config),
                    store,
                    runtime,
                    quitting: AtomicBool::new(false),
                    startup_warning: loaded.warning,
                });

                let open = MenuItem::with_id(app, "open", "Open Handy", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "Quit Handy", true, None::<&str>)?;
                let menu = Menu::with_items(app, &[&open, &quit])?;
                let mut tray = TrayIconBuilder::new()
                    .tooltip("Handy")
                    .menu(&menu)
                    .show_menu_on_left_click(true)
                    .on_menu_event(|app, event| match event.id.as_ref() {
                        "open" => show_main(app),
                        "quit" => begin_quit(app),
                        _ => {}
                    });
                if let Some(icon) = app.default_window_icon() {
                    tray = tray.icon(icon.clone());
                }
                tray.build(app)?;
                Ok(())
            })
            .on_window_event(|window, event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    let state = window.state::<AppState>();
                    if !state.quitting.load(Ordering::SeqCst) {
                        api.prevent_close();
                        let _ = window.hide();
                    }
                }
            })
            .invoke_handler(tauri::generate_handler![
                get_snapshot,
                detect_commands,
                save_project,
                delete_project,
                save_group,
                delete_group,
                start_target,
                stop_target,
                clear_logs,
            ])
            .build(tauri::generate_context!())
            .expect("error while building Handy")
            .run(|app, event| {
                if let tauri::RunEvent::ExitRequested { api, .. } = event {
                    let state = app.state::<AppState>();
                    if !state.quitting.load(Ordering::SeqCst) {
                        api.prevent_exit();
                        begin_quit(app);
                    }
                }
            });
    }
}

#[cfg(feature = "desktop")]
pub use desktop::run;

#[cfg(not(feature = "desktop"))]
pub fn run() {}
