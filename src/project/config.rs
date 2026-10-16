use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

use crate::project::registry::KnownDependency;
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    #[serde(default)]
    pub library_paths: Vec<String>,
    #[serde(default = "default_backend")]
    pub default_backend: String,
    #[serde(default = "default_compiler")]
    pub cxx_compiler: String,
    #[serde(default)]
    pub std_path: Option<String>,
    #[serde(default)]
    pub packages_dir: Option<String>,
    #[serde(default)]
    pub dependencies: HashMap<String, KnownDependency>,
}

fn default_backend() -> String {
    "cpp".to_string()
}

fn default_compiler() -> String {
    "g++".to_string()
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            library_paths: Vec::new(),
            default_backend: default_backend(),
            cxx_compiler: default_compiler(),
            std_path: None,
            packages_dir: None,
            dependencies: HashMap::new(),
        }
    }
}

impl GlobalConfig {
    pub fn config_path() -> PathBuf {
        if let Ok(custom) = std::env::var("FAST_CONFIG_PATH") {
            return PathBuf::from(custom);
        }

        let base_dir = dirs_fallback();
        let fast_dir = base_dir.join(".fast");
        fast_dir.join("config.toml")
    }

    pub fn load() -> Self {
        let p = Self::config_path();
        if !p.exists() {
            return GlobalConfig::default();
        }
        match fs::read_to_string(&p) {
            Ok(content) => toml::from_str(&content).unwrap_or_default(),
            Err(_) => GlobalConfig::default(),
        }
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::config_path();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&p, content).map_err(|e| e.to_string())
    }

    pub fn add_library_path(&mut self, path: &Path) -> Result<(), String> {
        let canonical = path
            .canonicalize()
            .unwrap_or_else(|_| path.to_path_buf())
            .to_string_lossy()
            .to_string();
        if !self.library_paths.contains(&canonical) {
            self.library_paths.push(canonical);
            self.save()?;
        }
        Ok(())
    }

    pub fn remove_library_path(&mut self, path_str: &str) -> Result<bool, String> {
        let before_len = self.library_paths.len();
        self.library_paths.retain(|p| p != path_str && !p.ends_with(path_str));
        let changed = self.library_paths.len() != before_len;
        if changed {
            self.save()?;
        }
        Ok(changed)
    }

    pub fn get_resolved_paths(&self) -> Vec<PathBuf> {
        self.library_paths
            .iter()
            .map(PathBuf::from)
            .filter(|p| p.exists())
            .collect()
    }
}

fn dirs_fallback() -> PathBuf {
    if let Ok(home) = std::env::var("USERPROFILE") {
        PathBuf::from(home)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else {
        std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
    }
}
