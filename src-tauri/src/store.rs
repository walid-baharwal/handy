use crate::model::Config;
use std::fs;
use std::path::{Path, PathBuf};

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

    pub fn load(&self) -> Config {
        self.load_file(&self.path)
            .or_else(|| self.load_file(&self.backup_path()))
            .unwrap_or_else(Config::empty)
    }

    pub fn save(&self, config: &Config) -> Result<(), String> {
        config.validate()?;
        let bytes = serde_json::to_vec_pretty(config).map_err(|error| error.to_string())?;
        let temporary = self.path.with_extension("json.tmp");
        fs::write(&temporary, bytes).map_err(|error| error.to_string())?;
        if self.path.exists() {
            fs::copy(&self.path, self.backup_path()).map_err(|error| error.to_string())?;
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

    fn load_file(&self, path: &Path) -> Option<Config> {
        let config: Config = serde_json::from_slice(&fs::read(path).ok()?).ok()?;
        config.validate().ok()?;
        Some(config)
    }
}
