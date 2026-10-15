//! Config and session persistence. See docs/technical-design.md section 9.
//!
//! Two JSON files in the app config dir. Reads are tolerant: a missing or
//! corrupt file falls back to the defaults rather than failing to start.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, State};

use crate::error::{AppError, Result};

const CONFIG_FILE: &str = "config.json";
const SESSION_FILE: &str = "session.json";

/// Fields missing from an older file take their defaults, so adding a setting
/// never resets the ones a user already has.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Config {
    pub version: u32,
    pub shell: Option<String>,
    pub shell_args: Vec<String>,
    pub notes_root: Option<PathBuf>,
    pub agent_patterns: Vec<String>,
    pub idle_timeout_ms: u64,
    pub hotkey_modifier: String,
    pub font_family: Option<String>,
    pub font_size: u32,
    pub scrollback: u32,
    pub restore_session_on_launch: bool,
    /// "system", "light", "dark", "black" or "glass" (macOS only).
    pub theme: String,
    /// Square, borderless panes with minimal gaps.
    pub compact_layout: bool,
    /// `#rrggbb` for the focused pane's ring; None follows the theme.
    pub focus_color: Option<String>,
    /// `#rrggbb` for a pane whose agent finished; None follows the theme.
    pub finished_color: Option<String>,
    /// Process triggers: the master switch and which definitions are armed.
    pub triggers: TriggersConfig,
}

/// Process triggers, kept in `config.json` and never in the session or the
/// cloud (plans/001-trigger-core/epic.md Decision 11).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct TriggersConfig {
    pub enabled: bool,
    /// Ids of the local process definitions the user has armed.
    pub armed: Vec<String>,
}

/// A partial update of `TriggersConfig`; a missing field is left alone.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TriggersPatch {
    pub enabled: Option<bool>,
    pub armed: Option<Vec<String>>,
}

const THEMES: [&str; 5] = ["system", "light", "dark", "black", "glass"];

impl Default for Config {
    fn default() -> Self {
        Self {
            version: 1,
            shell: None,
            shell_args: Vec::new(),
            notes_root: None,
            agent_patterns: ["claude", "codex", "gemini", "aider", "gpt", "copilot"]
                .iter()
                .map(|s| (*s).to_string())
                .collect(),
            idle_timeout_ms: 4000,
            hotkey_modifier: "ctrl+alt".to_string(),
            font_family: None,
            font_size: 14,
            scrollback: 10_000,
            restore_session_on_launch: true,
            theme: "system".to_string(),
            compact_layout: false,
            focus_color: None,
            finished_color: None,
            triggers: TriggersConfig::default(),
        }
    }
}

/// Every field optional so the frontend can send a partial update. For the
/// nullable settings a missing field leaves the value alone and `null` clears
/// it (`Some(None)`); serde alone reads both as None.
#[derive(Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigPatch {
    #[serde(default, deserialize_with = "nullable")]
    pub shell: Option<Option<String>>,
    pub shell_args: Option<Vec<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub notes_root: Option<Option<PathBuf>>,
    pub agent_patterns: Option<Vec<String>>,
    pub idle_timeout_ms: Option<u64>,
    pub hotkey_modifier: Option<String>,
    #[serde(default, deserialize_with = "nullable")]
    pub font_family: Option<Option<String>>,
    pub font_size: Option<u32>,
    pub scrollback: Option<u32>,
    pub restore_session_on_launch: Option<bool>,
    pub theme: Option<String>,
    pub compact_layout: Option<bool>,
    #[serde(default, deserialize_with = "nullable")]
    pub focus_color: Option<Option<String>>,
    #[serde(default, deserialize_with = "nullable")]
    pub finished_color: Option<Option<String>>,
    pub triggers: Option<TriggersPatch>,
}

/// A field that is present: `null` becomes `Some(None)`.
fn nullable<'de, T, D>(deserializer: D) -> std::result::Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// `#rrggbb` in lower case, or None for anything else.
fn hex_color(value: &str) -> Option<String> {
    let value = value.trim();
    let digits = value.strip_prefix('#')?;
    (digits.len() == 6 && digits.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| value.to_ascii_lowercase())
}

impl Config {
    fn apply(&mut self, patch: ConfigPatch) {
        if let Some(value) = patch.shell {
            self.shell = value.filter(|s| !s.trim().is_empty());
        }
        if let Some(value) = patch.shell_args {
            self.shell_args = value;
        }
        if let Some(value) = patch.notes_root {
            self.notes_root = value;
        }
        if let Some(value) = patch.agent_patterns {
            self.agent_patterns = value
                .into_iter()
                .map(|p| p.trim().to_lowercase())
                .filter(|p| !p.is_empty())
                .collect();
        }
        if let Some(value) = patch.idle_timeout_ms {
            self.idle_timeout_ms = value.clamp(500, 120_000);
        }
        if let Some(value) = patch.hotkey_modifier {
            if value == "ctrl+alt" || value == "ctrl+shift" {
                self.hotkey_modifier = value;
            }
        }
        if let Some(value) = patch.font_family {
            self.font_family = value.filter(|s| !s.trim().is_empty());
        }
        if let Some(value) = patch.font_size {
            self.font_size = value.clamp(8, 32);
        }
        if let Some(value) = patch.scrollback {
            self.scrollback = value.clamp(100, 200_000);
        }
        if let Some(value) = patch.restore_session_on_launch {
            self.restore_session_on_launch = value;
        }
        if let Some(value) = patch.theme {
            if THEMES.contains(&value.as_str()) {
                self.theme = value;
            }
        }
        if let Some(value) = patch.compact_layout {
            self.compact_layout = value;
        }
        // A value that isn't a colour is ignored rather than clearing the old one.
        match patch.focus_color {
            Some(None) => self.focus_color = None,
            Some(Some(value)) => self.focus_color = hex_color(&value).or(self.focus_color.take()),
            None => {}
        }
        match patch.finished_color {
            Some(None) => self.finished_color = None,
            Some(Some(value)) => {
                self.finished_color = hex_color(&value).or(self.finished_color.take())
            }
            None => {}
        }
        if let Some(triggers) = patch.triggers {
            if let Some(enabled) = triggers.enabled {
                self.triggers.enabled = enabled;
            }
            if let Some(armed) = triggers.armed {
                let mut seen = Vec::new();
                for id in armed {
                    if !id.is_empty() && !seen.contains(&id) {
                        seen.push(id);
                    }
                }
                self.triggers.armed = seen;
            }
        }
    }
}

/// Opaque to the core: the frontend owns the shape, we only round-trip it.
pub type Session = serde_json::Value;

pub struct StateService {
    dir: PathBuf,
    config: Arc<RwLock<Config>>,
}

impl StateService {
    pub fn new(dir: PathBuf) -> Self {
        let config = load_json(&dir.join(CONFIG_FILE)).unwrap_or_default();
        Self {
            dir,
            config: Arc::new(RwLock::new(config)),
        }
    }

    pub fn config(&self) -> Config {
        self.config.read().expect("config lock").clone()
    }

    pub fn set_config(&self, patch: ConfigPatch) -> Result<Config> {
        let updated = {
            let mut guard = self.config.write().expect("config lock");
            guard.apply(patch);
            guard.clone()
        };
        write_json(&self.dir.join(CONFIG_FILE), &updated)?;
        Ok(updated)
    }

    /// Used when a command changes a setting outside of `config_set`.
    pub fn update_config(&self, mutate: impl FnOnce(&mut Config)) -> Result<Config> {
        let updated = {
            let mut guard = self.config.write().expect("config lock");
            mutate(&mut guard);
            guard.clone()
        };
        write_json(&self.dir.join(CONFIG_FILE), &updated)?;
        Ok(updated)
    }

    pub fn session(&self) -> Option<Session> {
        load_json(&self.dir.join(SESSION_FILE))
    }

    pub fn set_session(&self, session: &Session) -> Result<()> {
        write_json(&self.dir.join(SESSION_FILE), session)
    }
}

fn load_json<T: serde::de::DeserializeOwned>(path: &Path) -> Option<T> {
    let raw = fs::read_to_string(path).ok()?;
    // A file edited by hand on Windows often arrives with a UTF-8 BOM, which
    // serde_json refuses; the settings should still load.
    let raw = raw.trim_start_matches('\u{feff}');
    match serde_json::from_str(raw) {
        Ok(value) => Some(value),
        Err(error) => {
            // A hand-edited or truncated file must not stop the app from starting.
            eprintln!("terminal-grid: ignoring {}: {error}", path.display());
            None
        }
    }
}

/// Written through a temp file so a crash mid-write cannot truncate the real one.
fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let json = serde_json::to_string_pretty(value).map_err(|e| AppError::Io {
        message: e.to_string(),
    })?;
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, json)?;
    fs::rename(&temp, path)?;
    Ok(())
}

/// `%APPDATA%\com.liamread.terminalgrid` on Windows.
pub fn config_dir(app: &AppHandle) -> Result<PathBuf> {
    let dir = app.path().app_config_dir().map_err(|e| AppError::Io {
        message: e.to_string(),
    })?;
    fs::create_dir_all(&dir)?;
    Ok(dir)
}

// Commands ----------------------------------------------------------------

#[tauri::command]
pub fn config_get(state: State<'_, crate::AppState>) -> Config {
    state.state.config()
}

#[tauri::command]
pub fn config_set(
    app: AppHandle,
    state: State<'_, crate::AppState>,
    patch: ConfigPatch,
) -> Result<Config> {
    let notes_root = patch.notes_root.clone().flatten();
    let config = state.state.set_config(patch)?;
    if let Some(root) = notes_root {
        // Keep the notes service in step with the setting it was saved from.
        state.notes.set_root(root)?;
    }
    crate::triggers::refresh_app(&app);
    Ok(config)
}

#[tauri::command]
pub fn session_get(state: State<'_, crate::AppState>) -> Option<Session> {
    state.state.session()
}

#[tauri::command]
pub fn session_set(state: State<'_, crate::AppState>, session: Session) -> Result<()> {
    state.state.set_session(&session)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn patch_only_touches_the_fields_it_carries() {
        let mut config = Config::default();
        config.apply(ConfigPatch {
            font_size: Some(18),
            ..Default::default()
        });
        assert_eq!(config.font_size, 18);
        assert_eq!(config.scrollback, 10_000);
        assert!(config.restore_session_on_launch);
    }

    #[test]
    fn patch_clamps_and_normalises() {
        let mut config = Config::default();
        config.apply(ConfigPatch {
            font_size: Some(400),
            idle_timeout_ms: Some(1),
            hotkey_modifier: Some("meta+q".to_string()),
            agent_patterns: Some(vec!["  Claude  ".into(), "".into()]),
            shell: Some(Some("   ".into())),
            ..Default::default()
        });
        assert_eq!(config.font_size, 32);
        assert_eq!(config.idle_timeout_ms, 500);
        assert_eq!(config.hotkey_modifier, "ctrl+alt");
        assert_eq!(config.agent_patterns, vec!["claude".to_string()]);
        assert_eq!(config.shell, None);
    }

    #[test]
    fn appearance_patch_accepts_known_themes_only() {
        let mut config = Config::default();
        assert_eq!(config.theme, "system");
        config.apply(ConfigPatch {
            theme: Some("black".into()),
            compact_layout: Some(true),
            ..Default::default()
        });
        assert_eq!(config.theme, "black");
        assert!(config.compact_layout);
        config.apply(ConfigPatch {
            theme: Some("sepia".into()),
            ..Default::default()
        });
        assert_eq!(config.theme, "black");
    }

    #[test]
    fn a_config_written_before_new_settings_keeps_its_values() {
        let config: Config =
            serde_json::from_str(r#"{"version":1,"fontSize":18,"hotkeyModifier":"ctrl+shift"}"#)
                .expect("older config parses");
        assert_eq!(config.font_size, 18);
        assert_eq!(config.hotkey_modifier, "ctrl+shift");
        assert_eq!(config.theme, "system");
        assert!(!config.compact_layout);
    }

    #[test]
    fn a_file_with_a_utf8_bom_still_loads() {
        let dir = std::env::temp_dir().join(format!("terminal-grid-bom-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(CONFIG_FILE);
        fs::write(&path, "﻿{\"version\":1,\"fontSize\":21}").expect("write");

        let loaded: Option<serde_json::Value> = load_json(&path);
        assert_eq!(loaded.and_then(|v| v["fontSize"].as_u64()), Some(21));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn explicit_null_clears_an_optional_field() {
        let mut config = Config {
            font_family: Some("Consolas".into()),
            ..Config::default()
        };
        config.apply(ConfigPatch {
            font_family: Some(None),
            ..Default::default()
        });
        assert_eq!(config.font_family, None);
    }

    #[test]
    fn a_null_from_the_frontend_clears_an_optional_field() {
        let patch: ConfigPatch = serde_json::from_str(r#"{"shell":null}"#).expect("patch parses");
        let mut config = Config {
            shell: Some("zsh".into()),
            ..Config::default()
        };
        config.apply(patch);
        assert_eq!(config.shell, None);
    }

    #[test]
    fn colors_take_hex_only_and_null_resets_them() {
        let mut config = Config::default();
        let patch: ConfigPatch =
            serde_json::from_str(r##"{"focusColor":"#FF8800","finishedColor":"#00cc66"}"##)
                .expect("patch parses");
        config.apply(patch);
        assert_eq!(config.focus_color.as_deref(), Some("#ff8800"));
        assert_eq!(config.finished_color.as_deref(), Some("#00cc66"));

        let patch: ConfigPatch =
            serde_json::from_str(r#"{"focusColor":"orange"}"#).expect("patch parses");
        config.apply(patch);
        assert_eq!(config.focus_color.as_deref(), Some("#ff8800"));

        let patch: ConfigPatch =
            serde_json::from_str(r#"{"focusColor":null}"#).expect("patch parses");
        config.apply(patch);
        assert_eq!(config.focus_color, None);
        assert_eq!(config.finished_color.as_deref(), Some("#00cc66"));
    }

    #[test]
    fn a_missing_field_leaves_an_optional_setting_alone() {
        let patch: ConfigPatch = serde_json::from_str(r#"{"fontSize":16}"#).expect("patch parses");
        let mut config = Config {
            shell: Some("zsh".into()),
            ..Config::default()
        };
        config.apply(patch);
        assert_eq!(config.shell.as_deref(), Some("zsh"));
    }

    #[test]
    fn triggers_are_off_and_empty_by_default() {
        let config = Config::default();
        assert!(!config.triggers.enabled);
        assert!(config.triggers.armed.is_empty());
        let json = serde_json::to_value(&config).expect("config serialises");
        assert_eq!(json["triggers"]["enabled"], false);
        assert_eq!(json["triggers"]["armed"], serde_json::json!([]));
    }

    #[test]
    fn a_config_written_before_triggers_loads_with_them_off() {
        let config: Config =
            serde_json::from_str(r#"{"version":1,"fontSize":18}"#).expect("older config parses");
        assert_eq!(config.triggers, TriggersConfig::default());
    }

    #[test]
    fn a_triggers_patch_changes_only_the_fields_it_carries() {
        let mut config = Config::default();
        let patch: ConfigPatch =
            serde_json::from_str(r#"{"triggers":{"armed":["a","b"]}}"#).expect("patch parses");
        config.apply(patch);
        assert!(!config.triggers.enabled);
        assert_eq!(
            config.triggers.armed,
            vec!["a".to_string(), "b".to_string()]
        );

        let patch: ConfigPatch =
            serde_json::from_str(r#"{"triggers":{"enabled":true}}"#).expect("patch parses");
        config.apply(patch);
        assert!(config.triggers.enabled);
        assert_eq!(config.triggers.armed.len(), 2);
    }

    #[test]
    fn armed_ids_are_deduplicated_and_empty_ones_dropped() {
        let mut config = Config::default();
        config.apply(ConfigPatch {
            triggers: Some(TriggersPatch {
                enabled: None,
                armed: Some(vec!["a".into(), "".into(), "a".into(), "b".into()]),
            }),
            ..Default::default()
        });
        assert_eq!(
            config.triggers.armed,
            vec!["a".to_string(), "b".to_string()]
        );
    }
}
