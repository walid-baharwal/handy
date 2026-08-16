use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Component, Path};

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Config {
    #[serde(default = "schema_version")]
    pub schema_version: u32,
    #[serde(default)]
    pub projects: HashMap<String, Project>,
    #[serde(default)]
    pub commands: HashMap<String, HandyCommand>,
    #[serde(default)]
    pub groups: HashMap<String, Group>,
}

fn schema_version() -> u32 {
    1
}

impl Config {
    pub fn empty() -> Self {
        Self {
            schema_version: 1,
            ..Self::default()
        }
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err(format!(
                "Unsupported schema version {}",
                self.schema_version
            ));
        }

        for (id, project) in &self.projects {
            if id != &project.id || id.is_empty() || project.name.trim().is_empty() {
                return Err("Every project needs a stable id and a name".into());
            }
            if !Path::new(&project.base_dir).is_absolute() {
                return Err(format!(
                    "Project '{}' must use an absolute folder",
                    project.name
                ));
            }
            for command_id in &project.command_ids {
                let command = self.commands.get(command_id).ok_or_else(|| {
                    format!("Project '{}' refers to a missing command", project.name)
                })?;
                if command.project_id != project.id {
                    return Err(format!(
                        "Command '{}' belongs to another project",
                        command.name
                    ));
                }
            }
        }

        for (id, command) in &self.commands {
            if id != &command.id
                || id.is_empty()
                || command.name.trim().is_empty()
                || command.command.trim().is_empty()
            {
                return Err("Every command needs a stable id, name, and shell command".into());
            }
            if !self.projects.contains_key(&command.project_id) {
                return Err(format!("Command '{}' has no project", command.name));
            }
            if !safe_relative_path(&command.cwd) {
                return Err(format!(
                    "Command '{}' has an unsafe working directory",
                    command.name
                ));
            }
        }

        for (id, group) in &self.groups {
            if id != &group.id || id.is_empty() || group.name.trim().is_empty() {
                return Err("Every group needs a stable id and a name".into());
            }
            for target in &group.targets {
                self.ensure_target_exists(target)?;
            }
        }

        for id in self.groups.keys() {
            self.check_group_cycle(id, &mut HashSet::new(), &mut HashSet::new())?;
        }
        Ok(())
    }

    pub fn resolve(&self, target: &TargetRef) -> Result<HashSet<String>, String> {
        self.ensure_target_exists(target)?;
        let mut commands = HashSet::new();
        self.resolve_into(target, &mut commands, &mut HashSet::new())?;
        Ok(commands)
    }

    fn ensure_target_exists(&self, target: &TargetRef) -> Result<(), String> {
        let exists = match target.kind {
            TargetKind::Project => self.projects.contains_key(&target.id),
            TargetKind::Command => self.commands.contains_key(&target.id),
            TargetKind::Group => self.groups.contains_key(&target.id),
        };
        exists
            .then_some(())
            .ok_or_else(|| format!("Missing {:?} target '{}'", target.kind, target.id))
    }

    fn resolve_into(
        &self,
        target: &TargetRef,
        commands: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
    ) -> Result<(), String> {
        match target.kind {
            TargetKind::Command => {
                commands.insert(target.id.clone());
            }
            TargetKind::Project => {
                commands.extend(self.projects[&target.id].command_ids.iter().cloned());
            }
            TargetKind::Group => {
                if !visiting.insert(target.id.clone()) {
                    return Err("Group cycle detected".into());
                }
                for child in &self.groups[&target.id].targets {
                    self.resolve_into(child, commands, visiting)?;
                }
                visiting.remove(&target.id);
            }
        }
        Ok(())
    }

    fn check_group_cycle(
        &self,
        id: &str,
        visiting: &mut HashSet<String>,
        visited: &mut HashSet<String>,
    ) -> Result<(), String> {
        if visited.contains(id) {
            return Ok(());
        }
        if !visiting.insert(id.to_string()) {
            return Err(format!("Group cycle includes '{}'", self.groups[id].name));
        }
        for child in &self.groups[id].targets {
            if child.kind == TargetKind::Group {
                self.check_group_cycle(&child.id, visiting, visited)?;
            }
        }
        visiting.remove(id);
        visited.insert(id.to_string());
        Ok(())
    }
}

fn safe_relative_path(value: &str) -> bool {
    let path = Path::new(value);
    !path.is_absolute()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_) | Component::CurDir))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: String,
    pub name: String,
    pub base_dir: String,
    #[serde(default)]
    pub command_ids: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HandyCommand {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub command: String,
    #[serde(default = "default_cwd")]
    pub cwd: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status_command: Option<String>,
}

fn default_cwd() -> String {
    ".".into()
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Group {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub targets: Vec<TargetRef>,
}

#[derive(Clone, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TargetRef {
    pub kind: TargetKind,
    pub id: String,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, PartialEq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetKind {
    Project,
    Command,
    Group,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Config {
        let project = Project {
            id: "p".into(),
            name: "Web".into(),
            base_dir: "/tmp/web".into(),
            command_ids: vec!["api".into()],
        };
        let command = HandyCommand {
            id: "api".into(),
            project_id: "p".into(),
            name: "API".into(),
            command: "npm start".into(),
            cwd: ".".into(),
            stop_command: None,
            status_command: None,
        };
        Config {
            schema_version: 1,
            projects: HashMap::from([(project.id.clone(), project)]),
            commands: HashMap::from([(command.id.clone(), command)]),
            groups: HashMap::new(),
        }
    }

    #[test]
    fn resolves_nested_groups_without_duplicates() {
        let mut config = sample();
        config.groups.insert(
            "inner".into(),
            Group {
                id: "inner".into(),
                name: "Inner".into(),
                targets: vec![TargetRef {
                    kind: TargetKind::Command,
                    id: "api".into(),
                }],
            },
        );
        config.groups.insert(
            "outer".into(),
            Group {
                id: "outer".into(),
                name: "Outer".into(),
                targets: vec![
                    TargetRef {
                        kind: TargetKind::Group,
                        id: "inner".into(),
                    },
                    TargetRef {
                        kind: TargetKind::Project,
                        id: "p".into(),
                    },
                ],
            },
        );
        assert_eq!(
            config
                .resolve(&TargetRef {
                    kind: TargetKind::Group,
                    id: "outer".into()
                })
                .unwrap()
                .len(),
            1
        );
    }

    #[test]
    fn rejects_group_cycles() {
        let mut config = sample();
        config.groups.insert(
            "a".into(),
            Group {
                id: "a".into(),
                name: "A".into(),
                targets: vec![TargetRef {
                    kind: TargetKind::Group,
                    id: "b".into(),
                }],
            },
        );
        config.groups.insert(
            "b".into(),
            Group {
                id: "b".into(),
                name: "B".into(),
                targets: vec![TargetRef {
                    kind: TargetKind::Group,
                    id: "a".into(),
                }],
            },
        );
        assert!(config.validate().unwrap_err().contains("cycle"));
    }

    #[test]
    fn rejects_working_directory_escape() {
        let mut config = sample();
        config.commands.get_mut("api").unwrap().cwd = "../secret".into();
        assert!(config.validate().unwrap_err().contains("unsafe"));
    }
}
