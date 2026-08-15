use crate::model::Config;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub struct LoadOutcome {
    pub config: Config,
    pub warning: Option<String>,
}

pub struct ConfigStore {
    path: PathBuf,
}

impl ConfigStore {
    pub fn new(app_data_dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&app_data_dir).map_err(|error| error.to_string())?;
        Ok(Self {
            path: app_data_dir.join("config.v1.json"),
        })
    }

    pub fn load(&self) -> LoadOutcome {
        if let Some(config) = self.load_file(&self.path) {
            return LoadOutcome {
                config,
                warning: None,
            };
        }

        let backup = self.backup_path();
        if let Some(config) = self.load_file(&backup) {
            return LoadOutcome {
                config,
                warning: Some(
                    "Configuration was recovered from backup; the unreadable primary file will be preserved on the next save"
                        .into(),
                ),
            };
        }

        let warning = (self.path.exists() || backup.exists()).then(|| {
            "Handy could not read its configuration or backup; the files were preserved and an empty configuration was loaded"
                .into()
        });
        LoadOutcome {
            config: Config::empty(),
            warning,
        }
    }

    pub fn save(&self, config: &Config) -> Result<(), String> {
        config.validate()?;
        let bytes = serde_json::to_vec_pretty(config).map_err(|error| error.to_string())?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        if self.path.exists() {
            let destination = if self.load_file(&self.path).is_some() {
                self.backup_path()
            } else {
                self.corrupt_path()
            };
            fs::copy(&self.path, destination).map_err(|error| error.to_string())?;
        }
        if let Err(first_error) = fs::rename(&temporary, &self.path) {
            if self.path.exists() {
                fs::remove_file(&self.path).map_err(|error| error.to_string())?;
                fs::rename(&temporary, &self.path).map_err(|error| error.to_string())?;
            } else {
                return Err(first_error.to_string());
            }
        }
        Ok(())
    }

    fn backup_path(&self) -> PathBuf {
        self.path.with_extension("json.bak")
    }

    fn corrupt_path(&self) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        self.path.with_extension(format!("json.corrupt-{timestamp}"))
    }

    fn load_file(&self, path: &Path) -> Option<Config> {
        let config: Config = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        config.validate().ok()?;
        Some(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store(name: &str) -> (ConfigStore, PathBuf) {
        let directory = std::env::temp_dir().join(format!(
            "handy-store-{name}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        (ConfigStore::new(directory.clone()).unwrap(), directory)
    }

    #[test]
    fn fresh_store_has_no_recovery_warning() {
        let (store, directory) = store("fresh");
        assert!(store.load().warning.is_none());
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn valid_backup_is_recovered_without_overwriting_it() {
        let (store, directory) = store("backup");
        fs::write(&store.path, "invalid").unwrap();
        let valid = serde_json::to_vec(&Config::empty()).unwrap();
        fs::write(store.backup_path(), &valid).unwrap();

        let loaded = store.load();
        assert!(loaded.warning.is_some());
        store.save(&loaded.config).unwrap();

        assert!(store.load_file(&store.path).is_some());
        assert_eq!(fs::read(store.backup_path()).unwrap(), valid);
        assert!(fs::read_dir(&directory).unwrap().any(|entry| {
            entry
                .unwrap()
                .file_name()
                .to_string_lossy()
                .contains(".corrupt-")
        }));
        fs::remove_dir_all(directory).unwrap();
    }

    #[test]
    fn unreadable_primary_and_backup_load_empty_with_warning() {
        let (store, directory) = store("invalid");
        fs::write(&store.path, "invalid").unwrap();
        fs::write(store.backup_path(), "also invalid").unwrap();

        let loaded = store.load();
        assert!(loaded.warning.is_some());
        assert_eq!(loaded.config.schema_version, 1);
        fs::remove_dir_all(directory).unwrap();
    }
}
