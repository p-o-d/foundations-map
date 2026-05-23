//! Persistent app settings stored via eframe's Storage trait.

use serde::{Deserialize, Serialize};

const STORAGE_KEY: &str = "foundations-map-settings";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    /// X4 locale ID — 44 (English) by default.
    pub locale: u32,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self { locale: 44 }
    }
}

/// Load persisted settings. Falls back to `AppSettings::default()` if storage
/// is absent (first run, or no persistence backend available).
pub fn load(storage: Option<&dyn eframe::Storage>) -> AppSettings {
    storage
        .and_then(|s| s.get_string(STORAGE_KEY))
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

/// Persist settings. Called from `eframe::App::save`.
pub fn save(storage: &mut dyn eframe::Storage, s: &AppSettings) {
    if let Ok(json) = serde_json::to_string(s) {
        storage.set_string(STORAGE_KEY, json);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_locale_is_english() {
        let s = AppSettings::default();
        assert_eq!(s.locale, 44);
    }

    #[test]
    fn settings_serde_roundtrip() {
        let s = AppSettings { locale: 49 };
        let json = serde_json::to_string(&s).unwrap();
        let back: AppSettings = serde_json::from_str(&json).unwrap();
        assert_eq!(back.locale, 49);
    }
}
