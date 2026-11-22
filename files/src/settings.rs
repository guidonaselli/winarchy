use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Default, PartialEq, Debug)]
pub struct Settings {
    #[serde(default)]
    pub show_hidden: bool,
    #[serde(default)]
    pub sort: String,
    #[serde(default)]
    pub descending: bool,
    #[serde(default)]
    pub language: String,
    #[serde(default)]
    pub hide_extensions: bool,
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub group: String,
    #[serde(default)]
    pub size_width: f64,
    #[serde(default)]
    pub modified_width: f64,
    #[serde(default)]
    pub pinned: Vec<PathBuf>,
    #[serde(default)]
    pub tags: std::collections::BTreeMap<PathBuf, Vec<String>>,
}

fn location() -> Option<PathBuf> {
    std::env::var_os("WINARCHY_FILES_SETTINGS")
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("LOCALAPPDATA")
                .map(|base| PathBuf::from(base).join("winarchy-files").join("settings.json"))
        })
}

// A missing or unreadable file means defaults: settings must never stop the app from starting.
pub fn load() -> Settings {
    location()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) {
    let Some(path) = location() else { return };
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(text) = serde_json::to_string_pretty(settings) {
        let _ = std::fs::write(path, text);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tolerates_partial_and_invalid_json() {
        let partial: Settings = serde_json::from_str(r#"{"show_hidden": true}"#).unwrap();
        assert_eq!(partial, Settings { show_hidden: true, sort: String::new(), descending: false, language: String::new(), hide_extensions: false, layout: String::new(), group: String::new(), size_width: 0.0, modified_width: 0.0, pinned: Vec::new(), tags: Default::default() });
        assert!(serde_json::from_str::<Settings>("not json").is_err());
    }
}
