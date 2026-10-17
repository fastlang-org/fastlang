use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
#[derive(Default)]
pub enum ProjectType {
    #[default]
    Bin,
    #[serde(alias = "lib")]
    Module,
}


impl ProjectType {
    pub fn from_str_opt(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "bin" | "binary" | "app" => Some(ProjectType::Bin),
            "module" | "lib" | "library" => Some(ProjectType::Module),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            ProjectType::Bin => "bin",
            ProjectType::Module => "module",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageConfig {
    pub name: String,
    pub version: String,
    #[serde(default, rename = "type")]
    pub project_type: ProjectType,
    #[serde(default)]
    pub main: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Option<Vec<String>>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub backend: Option<String>,
    #[serde(default)]
    pub link_flags: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PathsConfig {
    #[serde(default)]
    pub packages: Option<String>,
    #[serde(default)]
    pub libraries: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FastManifest {
    pub package: PackageConfig,
    #[serde(default)]
    pub scripts: Option<HashMap<String, String>>,
    #[serde(default)]
    pub dependencies: Option<HashMap<String, String>>,
    #[serde(default)]
    pub paths: Option<PathsConfig>,
}

impl FastManifest {
    pub const MANIFEST_FILENAME: &'static str = "Fast.toml";

    pub fn new(name: String, project_type: ProjectType) -> Self {
        let main_file = match project_type {
            ProjectType::Bin => "src/main.fast",
            ProjectType::Module => "src/lib.fast",
        };

        FastManifest {
            package: PackageConfig {
                name,
                version: "0.1.0".to_string(),
                project_type,
                main: Some(main_file.to_string()),
                description: None,
                authors: None,
                license: Some("MIT".to_string()),
                backend: Some("cpp".to_string()),
                link_flags: None,
            },
            scripts: Some(HashMap::new()),
            dependencies: Some(HashMap::new()),
            paths: Some(PathsConfig {
                packages: Some("packages".to_string()),
                libraries: Some(Vec::new()),
            }),
        }
    }

    pub fn load_from_file(path: &Path) -> Result<Self, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read manifest file '{}': {}", path.display(), e))?;
        toml::from_str(&content)
            .map_err(|e| format!("Failed to parse '{}': {}", path.display(), e))
    }

    pub fn save_to_file(&self, path: &Path) -> Result<(), String> {
        let content = toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize manifest: {}", e))?;
        fs::write(path, content)
            .map_err(|e| format!("Failed to write manifest to '{}': {}", path.display(), e))
    }

    pub fn find_in_dir_or_parents(start_dir: &Path) -> Option<(Self, PathBuf)> {
        let abs = if start_dir.is_absolute() {
            start_dir.to_path_buf()
        } else if let Ok(cwd) = std::env::current_dir() {
            cwd.join(start_dir)
        } else {
            start_dir.to_path_buf()
        };

        let mut curr = if abs.is_file() {
            abs.parent().unwrap_or(&abs).to_path_buf()
        } else {
            abs
        };

        loop {
            let manifest_path = curr.join(Self::MANIFEST_FILENAME);
            if manifest_path.is_file() {
                if let Ok(manifest) = Self::load_from_file(&manifest_path) {
                    return Some((manifest, curr));
                }
            }

            // Also check lowercase fast.toml for convenience
            let lowercase_path = curr.join("fast.toml");
            if lowercase_path.is_file() {
                if let Ok(manifest) = Self::load_from_file(&lowercase_path) {
                    return Some((manifest, curr));
                }
            }

            if !curr.pop() {
                break;
            }
        }
        None
    }

    pub fn get_packages_dir(&self, project_root: &Path) -> PathBuf {
        let rel = self
            .paths
            .as_ref()
            .and_then(|p| p.packages.as_deref())
            .unwrap_or("packages");
        project_root.join(rel)
    }

    pub fn get_entry_point(&self, project_root: &Path) -> PathBuf {
        if let Some(ref m) = self.package.main {
            return project_root.join(m);
        }
        match self.package.project_type {
            ProjectType::Bin => project_root.join("src").join("main.fast"),
            ProjectType::Module => project_root.join("src").join("lib.fast"),
        }
    }
}
