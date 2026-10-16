use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnownDependency {
    pub name: String,
    #[serde(default)]
    pub version: Option<String>,
    pub url: String,
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DependencyRegistry {
    #[serde(default)]
    pub dependencies: HashMap<String, KnownDependency>,
}

impl DependencyRegistry {
    pub fn config_path() -> PathBuf {
        if let Ok(custom) = std::env::var("FAST_CONFIG_PATH") {
            return PathBuf::from(custom);
        }
        let base_dir = dirs_fallback();
        let fast_dir = base_dir.join(".fast");
        fast_dir.join("config.toml")
    }

    pub fn load() -> Self {
        let mut reg = Self::default();
        let path = Self::config_path();
        if path.exists() {
            if let Ok(content) = fs::read_to_string(&path) {
                let _ = reg.import_from_str(&content);
            }
        }
        reg
    }

    pub fn save(&self) -> Result<(), String> {
        let p = Self::config_path();
        if let Some(parent) = p.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let content = toml::to_string_pretty(self).map_err(|e| e.to_string())?;
        fs::write(&p, content).map_err(|e| e.to_string())
    }

    pub fn get(&self, name: &str) -> Option<&KnownDependency> {
        self.dependencies.get(name)
    }

    pub fn add(&mut self, dep: KnownDependency) -> Result<(), String> {
        self.dependencies.insert(dep.name.clone(), dep);
        self.save()
    }

    pub fn list(&self) -> Vec<KnownDependency> {
        let mut list: Vec<KnownDependency> = self.dependencies.values().cloned().collect();
        list.sort_by(|a, b| a.name.cmp(&b.name));
        list
    }

    /// Import dependencies from a TOML string supporting:
    /// - Array format: `[[dependencies]]` or `[[packages]]` with `name`, `url`, `version`
    /// - Table format: `[dependencies]` where key is package name and value is table or URL
    /// - Simple array of names: `dependencies = ["sqlite3", "openssl"]`
    pub fn import_from_str(&mut self, toml_str: &str) -> Result<usize, String> {
        let value: toml::Value = toml::from_str(toml_str)
            .map_err(|e| format!("Failed to parse TOML dependency format: {}", e))?;

        let mut count = 0;

        if let Some(table) = value.as_table() {
            // Check for array of tables: [[dependencies]] or [[packages]]
            for key in &["dependencies", "packages", "dependency", "package"] {
                if let Some(arr) = table.get(*key).and_then(|v| v.as_array()) {
                    for item in arr {
                        if let Some(item_table) = item.as_table() {
                            if let Some(name) = item_table.get("name").and_then(|n| n.as_str()) {
                                let url = item_table
                                    .get("url")
                                    .and_then(|u| u.as_str())
                                    .unwrap_or("")
                                    .to_string();
                                let version = item_table
                                    .get("version")
                                    .and_then(|v| v.as_str())
                                    .map(|v| v.to_string());
                                let description = item_table
                                    .get("description")
                                    .and_then(|d| d.as_str())
                                    .map(|d| d.to_string());

                                self.dependencies.insert(
                                    name.to_string(),
                                    KnownDependency {
                                        name: name.to_string(),
                                        version,
                                        url,
                                        description,
                                    },
                                );
                                count += 1;
                            }
                        } else if let Some(pkg_name) = item.as_str() {
                            // Simple array item like "sqlite3"
                            if !self.dependencies.contains_key(pkg_name) {
                                self.dependencies.insert(
                                    pkg_name.to_string(),
                                    KnownDependency {
                                        name: pkg_name.to_string(),
                                        version: None,
                                        url: String::new(),
                                        description: None,
                                    },
                                );
                                count += 1;
                            }
                        }
                    }
                } else if let Some(dep_table) = table.get(*key).and_then(|v| v.as_table()) {
                    // Check for [dependencies] table format
                    for (name, val) in dep_table {
                        let (url, version, desc) = match val {
                            toml::Value::String(s) => {
                                if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("git") {
                                    (s.clone(), None, None)
                                } else {
                                    (String::new(), Some(s.clone()), None)
                                }
                            }
                            toml::Value::Table(t) => {
                                let u = t.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                                let v = t.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
                                let d = t.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
                                (u, v, d)
                            }
                            _ => (String::new(), None, None),
                        };

                        self.dependencies.insert(
                            name.clone(),
                            KnownDependency {
                                name: name.clone(),
                                version,
                                url,
                                description: desc,
                            },
                        );
                        count += 1;
                    }
                }
            }

            // Also check direct top-level key values (e.g. `sqlite3 = "3.36.0"`)
            for (key, val) in table {
                if key == "dependencies" || key == "packages" || key == "dependency" || key == "package" {
                    continue;
                }
                match val {
                    toml::Value::String(s) => {
                        let (url, ver) = if s.starts_with("http://") || s.starts_with("https://") || s.starts_with("git") {
                            (s.clone(), None)
                        } else {
                            (String::new(), Some(s.clone()))
                        };
                        self.dependencies.insert(
                            key.clone(),
                            KnownDependency {
                                name: key.clone(),
                                version: ver,
                                url,
                                description: None,
                            },
                        );
                        count += 1;
                    }
                    toml::Value::Table(t) => {
                        let u = t.get("url").and_then(|v| v.as_str()).unwrap_or("").to_string();
                        let v = t.get("version").and_then(|v| v.as_str()).map(|s| s.to_string());
                        let d = t.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
                        self.dependencies.insert(
                            key.clone(),
                            KnownDependency {
                                name: key.clone(),
                                version: v,
                                url: u,
                                description: d,
                            },
                        );
                        count += 1;
                    }
                    _ => {}
                }
            }
        }

        Ok(count)
    }

    pub fn import_file(&mut self, path: &Path) -> Result<usize, String> {
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Could not read registry file '{}': {}", path.display(), e))?;
        let count = self.import_from_str(&content)?;
        self.save()?;
        Ok(count)
    }
}

/// Download, cache, and resolve a known dependency into packages_dir
pub fn resolve_and_fetch_dependency(
    name: &str,
    req_version: Option<&str>,
    packages_dir: &Path,
) -> Result<PathBuf, String> {
    let target_dir = packages_dir.join(name);
    if target_dir.exists() {
        // Idempotent: already present, 0ms, skip
        return Ok(target_dir);
    }

    let registry = DependencyRegistry::load();
    let known = registry.get(name).ok_or_else(|| {
        format!(
            "Package '{}' is not registered in the known dependency registry.\nUse 'fast registry add {} <url>' to register it.",
            name, name
        )
    })?;

    fs::create_dir_all(&target_dir).map_err(|e| {
        format!("Failed to create package directory '{}': {}", target_dir.display(), e)
    })?;

    println!(
        "Resolving dependency '{}' (version: {})...",
        name,
        req_version.or(known.version.as_deref()).unwrap_or("latest")
    );

    // Special offline handling for bundled SQLite3 or C libraries:
    // If standard library has pre-bundled sqlite3, copy it directly
    if name == "sqlite3" {
        let std_dirs = crate::loader::get_std_dirs();
        let bundled_found = std_dirs.iter().any(|d| {
            let clib = d.join("clib");
            let c_file = clib.join("sqlite3.c");
            let h_file = clib.join("sqlite3.h");
            if c_file.exists() && h_file.exists() {
                let _ = fs::copy(&c_file, target_dir.join("sqlite3.c"));
                let _ = fs::copy(&h_file, target_dir.join("sqlite3.h"));
                let o_file = clib.join("sqlite3.o");
                if o_file.exists() {
                    let _ = fs::copy(&o_file, target_dir.join("sqlite3.o"));
                }
                true
            } else {
                false
            }
        });

        if bundled_found {
            println!("  Package '{}' installed from local cache into '{}'", name, target_dir.display());
            return Ok(target_dir);
        }
    }

    // Network download using curl if URL is present
    if !known.url.is_empty() {
        println!("  Downloading '{}' from {}...", name, known.url);
        let temp_archive = packages_dir.join(format!("{}_tmp.zip", name));
        
        let curl_status = Command::new("curl")
            .arg("-L")
            .arg("-o")
            .arg(&temp_archive)
            .arg(&known.url)
            .status();

        match curl_status {
            Ok(s) if s.success() => {
                // Extract using tar (built into Windows 10+ and Unix)
                let tar_status = Command::new("tar")
                    .arg("-xf")
                    .arg(&temp_archive)
                    .arg("-C")
                    .arg(&target_dir)
                    .status();

                let _ = fs::remove_file(&temp_archive);

                if let Ok(ts) = tar_status {
                    if ts.success() {
                        println!("  Package '{}' extracted successfully to '{}'", name, target_dir.display());
                        return Ok(target_dir);
                    }
                }

                // Fallback to powershell Expand-Archive on Windows
                if cfg!(windows) {
                    let ps_cmd = format!(
                        "Expand-Archive -Path '{}' -DestinationPath '{}' -Force",
                        temp_archive.display(),
                        target_dir.display()
                    );
                    let _ = Command::new("powershell")
                        .arg("-NoProfile")
                        .arg("-Command")
                        .arg(&ps_cmd)
                        .status();
                    let _ = fs::remove_file(&temp_archive);
                }
            }
            _ => {
                let _ = fs::remove_file(&temp_archive);
                return Err(format!("Failed to download package '{}' from URL '{}'", name, known.url));
            }
        }
    }

    Ok(target_dir)
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
