use crate::openai::{OpenAiConfig, DEFAULT_BASE_URL, DEFAULT_MODEL};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Saved model connection. Lives in the app data dir, not in the repo.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LlmSettings {
    #[serde(default)]
    pub api_key: String,
    #[serde(default)]
    pub model: String,
    #[serde(default)]
    pub base_url: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LlmKeySource {
    Env,
    Saved,
    None,
}

impl LlmSettings {
    pub fn path(data_dir: impl AsRef<Path>) -> PathBuf {
        data_dir.as_ref().join("llm.json")
    }

    pub fn load(data_dir: impl AsRef<Path>) -> Self {
        let path = Self::path(data_dir);
        let Ok(raw) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        serde_json::from_str(&raw).unwrap_or_default()
    }

    pub fn save(&self, data_dir: impl AsRef<Path>) -> Result<(), String> {
        let dir = data_dir.as_ref();
        std::fs::create_dir_all(dir).map_err(|e| format!("could not write coach settings: {e}"))?;
        let raw = serde_json::to_string_pretty(self)
            .map_err(|e| format!("could not write coach settings: {e}"))?;
        std::fs::write(Self::path(dir), raw)
            .map_err(|e| format!("could not write coach settings: {e}"))
    }

    pub fn clear(data_dir: impl AsRef<Path>) -> Result<(), String> {
        let path = Self::path(data_dir);
        if path.exists() {
            std::fs::remove_file(&path)
                .map_err(|e| format!("could not clear coach settings: {e}"))?;
        }
        Ok(())
    }

    pub fn has_key(&self) -> bool {
        !self.api_key.trim().is_empty()
    }

    pub fn apply_update(&mut self, api_key: Option<String>, model: String, base_url: String) {
        if let Some(key) = api_key {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                self.api_key = trimmed.to_string();
            }
        }
        self.model = model.trim().to_string();
        self.base_url = base_url.trim().trim_end_matches('/').to_string();
    }
}

/// Last four characters of a key, for the HUD. Never the full secret.
pub fn key_hint(key: &str) -> Option<String> {
    let trimmed = key.trim();
    if trimmed.is_empty() {
        return None;
    }
    let n = trimmed.chars().count();
    let tail: String = trimmed.chars().skip(n.saturating_sub(4)).collect();
    Some(format!("…{tail}"))
}

pub fn env_api_key() -> Option<String> {
    first_env(&["OPTCG_LLM_API_KEY", "OPENAI_API_KEY"])
}

pub fn env_model() -> Option<String> {
    first_env(&["OPTCG_LLM_MODEL", "OPENAI_MODEL"])
}

pub fn env_base_url() -> Option<String> {
    first_env(&["OPTCG_LLM_BASE_URL", "OPENAI_BASE_URL"])
}

fn first_env(keys: &[&str]) -> Option<String> {
    keys.iter()
        .filter_map(|key| std::env::var(key).ok())
        .map(|value| value.trim().to_string())
        .find(|value| !value.is_empty())
}

pub fn key_source(saved: &LlmSettings) -> LlmKeySource {
    if env_api_key().is_some() {
        LlmKeySource::Env
    } else if saved.has_key() {
        LlmKeySource::Saved
    } else {
        LlmKeySource::None
    }
}

/// Merge a saved file with the process environment. Environment wins.
pub fn resolve_config(saved: &LlmSettings) -> OpenAiConfig {
    resolve_with(
        saved,
        env_api_key().as_deref(),
        env_model().as_deref(),
        env_base_url().as_deref(),
    )
}

pub fn resolve_with(
    saved: &LlmSettings,
    env_key: Option<&str>,
    env_model: Option<&str>,
    env_url: Option<&str>,
) -> OpenAiConfig {
    let mut config = OpenAiConfig::default();
    if saved.has_key() {
        config.api_key = saved.api_key.trim().to_string();
    }
    if !saved.model.trim().is_empty() {
        config.model = saved.model.trim().to_string();
    }
    if !saved.base_url.trim().is_empty() {
        config.base_url = saved.base_url.trim().trim_end_matches('/').to_string();
    }
    if let Some(key) = env_key.map(str::trim).filter(|s| !s.is_empty()) {
        config.api_key = key.to_string();
    }
    if let Some(model) = env_model.map(str::trim).filter(|s| !s.is_empty()) {
        config.model = model.to_string();
    }
    if let Some(url) = env_url.map(str::trim).filter(|s| !s.is_empty()) {
        config.base_url = url.trim_end_matches('/').to_string();
    }
    if config.model.is_empty() {
        config.model = DEFAULT_MODEL.to_string();
    }
    if config.base_url.is_empty() {
        config.base_url = DEFAULT_BASE_URL.to_string();
    }
    config
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    fn temp_dir() -> PathBuf {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!("optcg-llm-{}-{n}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_file_loads_empty() {
        let dir = temp_dir();
        assert_eq!(LlmSettings::load(&dir), LlmSettings::default());
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn round_trips_through_disk() {
        let dir = temp_dir();
        let saved = LlmSettings {
            api_key: "sk-test-key".into(),
            model: "gpt-4o-mini".into(),
            base_url: "https://api.openai.com/v1".into(),
        };
        saved.save(&dir).unwrap();
        assert_eq!(LlmSettings::load(&dir), saved);
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn empty_key_on_update_keeps_the_saved_one() {
        let mut saved = LlmSettings {
            api_key: "sk-keep".into(),
            model: "old".into(),
            base_url: "".into(),
        };
        saved.apply_update(None, "gpt-4o-mini".into(), "https://api.openai.com/v1".into());
        assert_eq!(saved.api_key, "sk-keep");
        assert_eq!(saved.model, "gpt-4o-mini");
    }

    #[test]
    fn env_key_wins_over_saved() {
        let saved = LlmSettings {
            api_key: "sk-saved".into(),
            model: "saved-model".into(),
            base_url: "http://localhost:11434/v1".into(),
        };
        let config = resolve_with(
            &saved,
            Some("sk-env"),
            Some("gpt-4o-mini"),
            Some("https://api.openai.com/v1/"),
        );
        assert_eq!(config.api_key, "sk-env");
        assert_eq!(config.model, "gpt-4o-mini");
        assert_eq!(config.base_url, "https://api.openai.com/v1");
    }

    #[test]
    fn saved_key_is_used_when_env_is_empty() {
        let saved = LlmSettings {
            api_key: "sk-saved".into(),
            ..Default::default()
        };
        let config = resolve_with(&saved, None, None, None);
        assert_eq!(config.api_key, "sk-saved");
        assert_eq!(config.model, DEFAULT_MODEL);
        assert_eq!(config.base_url, DEFAULT_BASE_URL);
    }

    #[test]
    fn key_hint_shows_only_the_tail() {
        assert_eq!(key_hint("sk-abcdefgh").as_deref(), Some("…efgh"));
        assert_eq!(key_hint("   ").as_deref(), None);
    }
}
