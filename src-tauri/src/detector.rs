use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CommandSuggestion {
    name: String,
    command: String,
    cwd: String,
    source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    stop_command: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    status_command: Option<String>,
}

pub fn detect(base_dir: &str) -> Result<Vec<CommandSuggestion>, String> {
    let root = Path::new(base_dir);
    if !root.is_absolute() || !root.is_dir() {
        return Err("Choose an existing project folder".into());
    }

    let mut suggestions = Vec::new();
    let package_path = root.join("package.json");
    if package_path.is_file() {
        let package: Value =
            serde_json::from_slice(&fs::read(package_path).map_err(|error| error.to_string())?)
                .map_err(|error| format!("Invalid package.json: {error}"))?;
        if let Some(scripts) = package.get("scripts").and_then(Value::as_object) {
            let manager = package_manager(root);
            let mut names: Vec<_> = scripts.keys().collect();
            names.sort();
            suggestions.extend(names.into_iter().map(|name| CommandSuggestion {
                name: name.to_string(),
                command: format!("{manager} run {name}"),
                cwd: ".".into(),
                source: "package.json".into(),
                stop_command: None,
                status_command: None,
            }));
        }
    }

    for filename in [
        "compose.yaml",
        "compose.yml",
        "docker-compose.yaml",
        "docker-compose.yml",
    ] {
        if root.join(filename).is_file() {
            suggestions.push(CommandSuggestion {
                name: format!("Docker ({filename})"),
                command: format!("docker compose -f {filename} up"),
                cwd: ".".into(),
                source: "Docker Compose".into(),
                stop_command: Some(format!("docker compose -f {filename} down")),
                status_command: Some(compose_status_command(filename)),
            });
        }
    }
    Ok(suggestions)
}

fn compose_status_command(filename: &str) -> String {
    #[cfg(windows)]
    return format!(
        "if (docker compose -f {filename} ps --status running --quiet) {{ exit 0 }} else {{ exit 1 }}"
    );

    #[cfg(not(windows))]
    format!("test -n \"$(docker compose -f {filename} ps --status running --quiet)\"")
}

fn package_manager(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").is_file() {
        "pnpm"
    } else if root.join("yarn.lock").is_file() {
        "yarn"
    } else if root.join("bun.lock").is_file() || root.join("bun.lockb").is_file() {
        "bun"
    } else {
        "npm"
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn compose_suggestion_includes_status_and_stop_commands() {
        let directory = std::env::temp_dir().join(format!(
            "handy-detect-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join("compose.yaml"), "services: {}\n").unwrap();

        let suggestion = detect(directory.to_str().unwrap()).unwrap().remove(0);

        assert_eq!(
            suggestion.stop_command.as_deref(),
            Some("docker compose -f compose.yaml down")
        );
        assert!(suggestion
            .status_command
            .as_deref()
            .unwrap()
            .contains("docker compose -f compose.yaml ps --status running --quiet"));
        fs::remove_dir_all(directory).unwrap();
    }
}
