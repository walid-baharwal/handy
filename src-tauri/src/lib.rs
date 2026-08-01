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
    use std::sync::{Arc, Mutex};
    use tauri::{Manager, State};

    struct AppState {
        config: Mutex<Config>,
        store: ConfigStore,
        runtime: Arc<RuntimeManager>,
    }

    #[derive(Serialize)]
    struct Snapshot {
        config: Config,
        runtime: Vec<RuntimeEntry>,
        logs: Vec<LogEntry>,
    }

    #[tauri::command]
    fn get_snapshot(state: State<'_, AppState>) -> Snapshot {
        Snapshot {
            config: state.config.lock().unwrap().clone(),
            runtime: state.runtime.snapshot(),
            logs: state.runtime.log_snapshot(),
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
        let mut next = config.clone();
        next.groups.insert(group.id.clone(), group);
        state.store.save(&next)?;
        *config = next.clone();
        Ok(next)
    }

    #[tauri::command]
    fn delete_group(id: String, state: State<'_, AppState>) -> Result<Config, String> {
        let mut config = state.config.lock().unwrap();
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
    fn clear_logs(state: State<'_, AppState>) {
        state.runtime.clear_logs();
    }

    #[cfg_attr(mobile, tauri::mobile_entry_point)]
    pub fn run() {
        tauri::Builder::default()
            .plugin(tauri_plugin_dialog::init())
            .setup(|app| {
                let store =
                    ConfigStore::new(app.path().app_data_dir()?).map_err(std::io::Error::other)?;
                let config = store.load();
                let runtime = Arc::new(RuntimeManager::new(app.handle().clone()));
                app.manage(AppState {
                    config: Mutex::new(config),
                    store,
                    runtime,
                });
                Ok(())
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
            .run(tauri::generate_context!())
            .expect("error while running Handy");
    }
}

#[cfg(feature = "desktop")]
pub use desktop::run;

#[cfg(not(feature = "desktop"))]
pub fn run() {}
