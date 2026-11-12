//! Impostazioni dell'applicazione e chiavi API.

pub mod keys;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

use crate::providers::{ProviderId, DEFAULT_LANGUAGE};
use crate::renamer::format::{DEFAULT_MOVIE_FORMAT, DEFAULT_SERIES_FORMAT};

/// Lingue selezionabili per titoli ed episodi (codici ISO 639-1).
pub const LANGUAGES: [&str; 7] = ["it", "en", "de", "fr", "es", "pt", "ja"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Theme {
    /// Segue il tema del sistema operativo.
    #[default]
    System,
    Light,
    Dark,
}

/// Preferenze salvate tra un avvio e l'altro. Non contiene mai le chiavi API.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub theme: Theme,
    /// Database principale.
    pub provider: ProviderId,
    /// Usa l'altro database quando il principale non trova risultati.
    pub use_fallback: bool,
    pub language: String,
    pub movie_format: String,
    pub series_format: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            theme: Theme::default(),
            provider: ProviderId::Tmdb,
            use_fallback: true,
            language: DEFAULT_LANGUAGE.to_string(),
            movie_format: DEFAULT_MOVIE_FORMAT.to_string(),
            series_format: DEFAULT_SERIES_FORMAT.to_string(),
        }
    }
}

/// Cartella di configurazione prevista dal sistema operativo.
pub fn config_dir() -> Option<PathBuf> {
    ProjectDirs::from("", "", "renbot").map(|dirs| dirs.config_dir().to_path_buf())
}

/// File di registro dell'ultima esecuzione.
pub fn log_path() -> Option<PathBuf> {
    ProjectDirs::from("", "", "renbot").map(|dirs| dirs.data_dir().join("renbot.log"))
}

impl Settings {
    pub fn default_path() -> Option<PathBuf> {
        config_dir().map(|dir| dir.join("settings.json"))
    }

    /// Legge le impostazioni; un file assente o illeggibile dà i valori predefiniti.
    pub fn load(path: &Path) -> Self {
        fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, serde_json::to_string_pretty(self)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn valori_predefiniti() {
        let settings = Settings::default();
        assert_eq!(settings.language, "it");
        assert_eq!(settings.provider, ProviderId::Tmdb);
        assert!(settings.use_fallback);
        assert_eq!(settings.theme, Theme::System);
    }

    #[test]
    fn salva_e_rilegge() {
        let dir = TempDir::new("settings");
        let path = dir.path().join("config").join("settings.json");
        assert_eq!(Settings::load(&path), Settings::default());

        let settings = Settings {
            theme: Theme::Dark,
            provider: ProviderId::Tvdb,
            use_fallback: false,
            language: "en".to_string(),
            movie_format: "{n}.{ext}".to_string(),
            series_format: "{n} {s00e00}.{ext}".to_string(),
        };
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path), settings);
    }

    #[test]
    fn un_file_parziale_o_rovinato_non_blocca_l_avvio() {
        let dir = TempDir::new("settings-partial");
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"theme": "Light"}"#).unwrap();
        let settings = Settings::load(&path);
        assert_eq!(settings.theme, Theme::Light);
        assert_eq!(settings.language, "it");

        fs::write(&path, "non è json").unwrap();
        assert_eq!(Settings::load(&path), Settings::default());
    }
}
