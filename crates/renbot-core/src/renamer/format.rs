//! Formato dei nuovi nomi: sostituzione dei token e pulizia per il file system.
//!
//! Nel formato la barra `/` separa le cartelle; il risultato è un percorso relativo
//! costruito con `PathBuf`, quindi valido su ogni sistema operativo.

use std::path::PathBuf;
use std::sync::LazyLock;

use regex::{Captures, Regex};

use crate::parser::ParsedName;
use crate::providers::{Episode, SearchResult};

pub const DEFAULT_MOVIE_FORMAT: &str = "{n} ({y})/{n} ({y}).{ext}";
pub const DEFAULT_SERIES_FORMAT: &str = "{n}/Season {s}/{n} - {s00e00} - {t}.{ext}";

/// Coppia di formati pronti all'uso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatPreset {
    pub movie: &'static str,
    pub series: &'static str,
}

/// Schemi predefiniti: con cartelle, solo nome del file, con qualità tra parentesi quadre.
pub const PRESETS: [FormatPreset; 3] = [
    FormatPreset {
        movie: DEFAULT_MOVIE_FORMAT,
        series: DEFAULT_SERIES_FORMAT,
    },
    FormatPreset {
        movie: "{n} ({y}).{ext}",
        series: "{n} - {s00e00} - {t}.{ext}",
    },
    FormatPreset {
        movie: "{n} ({y})/{n} ({y}) [{q}].{ext}",
        series: "{n}/Season {s}/{n} - {s00e00} - {t} [{q}].{ext}",
    },
];

/// Token riconosciuti dal formato.
pub const TOKENS: [&str; 9] = ["n", "y", "s", "e", "s00e00", "t", "ext", "q", "g"];

static TOKEN: LazyLock<Regex> = LazyLock::new(|| re(r"\{([^{}]*)\}"));
static EMPTY_BRACKETS: LazyLock<Regex> = LazyLock::new(|| re(r"\(\s*\)|\[\s*\]"));
static REPEATED_DASH: LazyLock<Regex> = LazyLock::new(|| re(r"( -){2,}"));
static DASH_BEFORE_EXTENSION: LazyLock<Regex> = LazyLock::new(|| re(r"[ -]+(\.[A-Za-z0-9]+)$"));
static RESERVED_NAME: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)^(con|prn|aux|nul|com[1-9]|lpt[1-9])(\.|$)"));

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("espressione regolare valida")
}

/// Valori con cui sostituire i token.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct FormatValues {
    /// `{n}`: titolo del film o della serie.
    pub name: String,
    /// `{y}`: anno.
    pub year: Option<u16>,
    /// `{s}`: stagione; con `{e}` forma anche `{s00e00}`.
    pub season: Option<u32>,
    /// `{e}`: episodio.
    pub episode: Option<u32>,
    /// `{t}`: titolo dell'episodio.
    pub title: String,
    /// `{q}`: qualità ricavata dal nome originale.
    pub quality: Option<String>,
    /// `{g}`: gruppo di release ricavato dal nome originale.
    pub group: Option<String>,
    /// `{ext}`: estensione senza punto.
    pub extension: String,
}

impl FormatValues {
    pub fn movie(parsed: &ParsedName, movie: &SearchResult) -> Self {
        Self {
            name: movie.title.clone(),
            year: movie.year,
            ..Self::from_file(parsed)
        }
    }

    pub fn episode(parsed: &ParsedName, series: &SearchResult, episode: &Episode) -> Self {
        Self {
            name: series.title.clone(),
            year: series.year,
            season: Some(episode.season),
            episode: Some(episode.number),
            title: episode.title.clone(),
            ..Self::from_file(parsed)
        }
    }

    fn from_file(parsed: &ParsedName) -> Self {
        Self {
            quality: parsed.quality.clone(),
            group: parsed.group.clone(),
            extension: parsed.extension.clone().unwrap_or_default(),
            ..Self::default()
        }
    }

    fn token(&self, name: &str) -> Option<String> {
        let number = |value: Option<u32>| value.map(|v| v.to_string()).unwrap_or_default();
        Some(match name {
            "n" => self.name.clone(),
            "y" => self.year.map(|y| y.to_string()).unwrap_or_default(),
            "s" => number(self.season),
            "e" => number(self.episode),
            "s00e00" => match (self.season, self.episode) {
                (Some(season), Some(episode)) => format!("S{season:02}E{episode:02}"),
                (None, Some(episode)) => format!("E{episode:02}"),
                _ => String::new(),
            },
            "t" => self.title.clone(),
            "q" => self.quality.clone().unwrap_or_default(),
            "g" => self.group.clone().unwrap_or_default(),
            "ext" => self.extension.clone(),
            _ => return None,
        })
    }
}

/// Token presenti nel formato ma non riconosciuti.
pub fn unknown_tokens(format: &str) -> Vec<String> {
    TOKEN
        .captures_iter(format)
        .map(|c| c[1].to_string())
        .filter(|name| !TOKENS.contains(&name.as_str()))
        .collect()
}

/// Applica il formato e restituisce il percorso relativo del nuovo file.
/// I token sconosciuti restano nel nome così come sono scritti.
pub fn render(format: &str, values: &FormatValues) -> PathBuf {
    format
        .split('/')
        .map(|component| {
            let filled = TOKEN.replace_all(component, |c: &Captures| {
                values.token(&c[1]).unwrap_or_else(|| c[0].to_string())
            });
            sanitize(&tidy(&sanitize(&filled)))
        })
        .filter(|component| !component.is_empty())
        .collect()
}

/// Rimuove i resti lasciati dai token vuoti: parentesi vuote e trattini isolati.
fn tidy(component: &str) -> String {
    let text = EMPTY_BRACKETS.replace_all(component, "");
    let text = collapse_spaces(&text);
    let text = REPEATED_DASH.replace_all(&text, " -");
    let text = DASH_BEFORE_EXTENSION.replace(&text, "$1");
    text.trim_start_matches([' ', '-'])
        .trim_end_matches([' ', '-'])
        .to_string()
}

/// Rende il nome valido su Windows, macOS e Linux: usa le regole più restrittive
/// (quelle di Windows) così la libreria resta leggibile da ogni sistema.
fn sanitize(component: &str) -> String {
    let cleaned: String = component
        .chars()
        .filter(|c| !c.is_control() && !r#"<>:"/\|?*"#.contains(*c))
        .collect();
    let cleaned = collapse_spaces(&cleaned);
    let cleaned = cleaned.trim_end_matches([' ', '.']);
    if RESERVED_NAME.is_match(cleaned) {
        format!("_{cleaned}")
    } else {
        cleaned.to_string()
    }
}

fn collapse_spaces(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;

    fn episode() -> FormatValues {
        FormatValues {
            name: "Show Name".to_string(),
            year: Some(2019),
            season: Some(1),
            episode: Some(2),
            title: "Titolo episodio".to_string(),
            quality: Some("1080p WEB-DL".to_string()),
            group: Some("GRP".to_string()),
            extension: "mkv".to_string(),
        }
    }

    fn movie() -> FormatValues {
        FormatValues {
            name: "Inception".to_string(),
            year: Some(2010),
            extension: "mkv".to_string(),
            ..FormatValues::default()
        }
    }

    fn path(parts: &[&str]) -> PathBuf {
        parts.iter().collect()
    }

    #[test]
    fn formati_predefiniti() {
        assert_eq!(
            render(DEFAULT_SERIES_FORMAT, &episode()),
            path(&[
                "Show Name",
                "Season 1",
                "Show Name - S01E02 - Titolo episodio.mkv"
            ])
        );
        assert_eq!(
            render(DEFAULT_MOVIE_FORMAT, &movie()),
            path(&["Inception (2010)", "Inception (2010).mkv"])
        );
    }

    #[test]
    fn tutti_i_token() {
        assert_eq!(
            render("{n} {y} {s} {e} {s00e00} {t} {q} {g}.{ext}", &episode()),
            Path::new("Show Name 2019 1 2 S01E02 Titolo episodio 1080p WEB-DL GRP.mkv")
        );
    }

    #[test]
    fn i_token_vuoti_non_lasciano_resti() {
        let mut values = movie();
        values.year = None;
        assert_eq!(
            render(DEFAULT_MOVIE_FORMAT, &values),
            path(&["Inception", "Inception.mkv"])
        );
        assert_eq!(
            render(PRESETS[2].movie, &values),
            path(&["Inception", "Inception.mkv"])
        );

        let mut values = episode();
        values.title.clear();
        assert_eq!(
            render("{n} - {s00e00} - {t}.{ext}", &values),
            Path::new("Show Name - S01E02.mkv")
        );
        assert_eq!(
            render("{n} - {t} - {s00e00}.{ext}", &values),
            Path::new("Show Name - S01E02.mkv")
        );
    }

    #[test]
    fn rimuove_i_caratteri_non_ammessi() {
        let mut values = movie();
        values.name = "Face/Off: \"What?\" <1997> *|\\".to_string();
        values.year = None;
        assert_eq!(
            render("{n}.{ext}", &values),
            Path::new("FaceOff What 1997.mkv")
        );
    }

    #[test]
    fn nomi_riservati_e_punti_finali() {
        let mut values = movie();
        values.name = "CON".to_string();
        assert_eq!(
            render("{n}/{n}.{ext}", &values),
            path(&["_CON", "_CON.mkv"])
        );
        values.name = "Finale...".to_string();
        assert_eq!(render("{n}/x.{ext}", &values), path(&["Finale", "x.mkv"]));
    }

    #[test]
    fn episodio_senza_stagione() {
        let mut values = episode();
        values.season = None;
        assert_eq!(
            render("{n} - {s00e00}.{ext}", &values),
            Path::new("Show Name - E02.mkv")
        );
    }

    #[test]
    fn token_sconosciuti() {
        assert_eq!(
            unknown_tokens("{n} {x} {anno}.{ext}"),
            ["x".to_string(), "anno".to_string()]
        );
        assert!(unknown_tokens(DEFAULT_SERIES_FORMAT).is_empty());
        assert_eq!(
            render("{n} {x}.{ext}", &movie()),
            Path::new("Inception {x}.mkv")
        );
    }

    #[test]
    fn i_preset_usano_solo_token_validi() {
        for preset in PRESETS {
            assert!(unknown_tokens(preset.movie).is_empty());
            assert!(unknown_tokens(preset.series).is_empty());
        }
    }
}
