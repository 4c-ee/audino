use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

/// Manages INI-style configuration from XDG_CONFIG_HOME/audino/audino.ini.
pub struct Config {
    sections: HashMap<String, HashMap<String, String>>,
    path: PathBuf,
}

impl Config {
    pub fn load() -> Self {
        let config_dir = Self::config_dir();
        let path = config_dir.join("audino.ini");

        let sections = if path.exists() {
            Self::parse_ini(&fs::read_to_string(&path).unwrap_or_default())
        } else {
            HashMap::new()
        };

        Self { sections, path }
    }

    pub fn config_dir() -> PathBuf {
        let base = std::env::var("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                PathBuf::from(home).join(".config")
            });
        base.join("audino")
    }

    pub fn ensure_config_dir(&self) {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent).ok();
        }
    }

    pub fn path(&self) -> &PathBuf {
        &self.path
    }

    pub fn get(&self, section: &str, key: &str) -> Option<String> {
        self.sections
            .get(section)
            .and_then(|s| s.get(key))
            .cloned()
    }

    pub fn set(&mut self, section: &str, key: &str, value: &str) {
        self.sections
            .entry(section.to_string())
            .or_default()
            .insert(key.to_string(), value.to_string());
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut contents = String::new();
        for (section, values) in &self.sections {
            contents.push_str(&format!("[{}]\n", section));
            for (key, value) in values {
                contents.push_str(&format!("{}={}\n", key, value));
            }
            contents.push('\n');
        }
        fs::write(&self.path, contents)
    }

    fn parse_ini(content: &str) -> HashMap<String, HashMap<String, String>> {
        let mut sections: HashMap<String, HashMap<String, String>> = HashMap::new();
        let mut current_section = String::new();

        for line in content.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') || line.starts_with(';') {
                continue;
            }

            if line.starts_with('[') && line.ends_with(']') {
                current_section = line[1..line.len() - 1].to_string();
                continue;
            }

            if let Some((key, value)) = line.split_once('=') {
                sections
                    .entry(current_section.clone())
                    .or_default()
                    .insert(key.trim().to_string(), value.trim().to_string());
            }
        }

        sections
    }
}
