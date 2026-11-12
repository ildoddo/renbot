//! Estrae titolo, anno, stagione, episodio, qualità e gruppo dal nome di un file video.

use std::path::Path;
use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Informazioni ricavate dal nome di un file.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ParsedName {
    /// Titolo ripulito dai separatori. Vuoto se il nome non ne contiene uno.
    pub title: String,
    pub year: Option<u16>,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// Risoluzione e sorgente, ad esempio `1080p WEB-DL`.
    pub quality: Option<String>,
    /// Gruppo di release.
    pub group: Option<String>,
    /// Estensione in minuscolo, senza punto.
    pub extension: Option<String>,
}

/// Tipo di contenuto dedotto dal nome.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MediaKind {
    Movie,
    Episode,
}

impl ParsedName {
    /// Un nome con un numero di episodio è un episodio, altrimenti un film.
    pub fn kind(&self) -> MediaKind {
        if self.episode.is_some() {
            MediaKind::Episode
        } else {
            MediaKind::Movie
        }
    }
}

fn re(pattern: &str) -> Regex {
    Regex::new(pattern).expect("espressione regolare valida")
}

static LEADING_GROUP: LazyLock<Regex> = LazyLock::new(|| re(r"^\s*\[([^\]]+)\]"));
static TRAILING_BRACKETS: LazyLock<Regex> = LazyLock::new(|| re(r"(?:\s*[\[(][^\])]*[\])])+\s*$"));
static TRAILING_GROUP: LazyLock<Regex> = LazyLock::new(|| re(r"-([A-Za-z0-9]+)$"));

/// Termini tecnici che non fanno mai parte del titolo, in qualsiasi grafia.
static TECH: LazyLock<Regex> = LazyLock::new(|| {
    re(concat!(
        r"(?i)\b(?:2160p|1080[pi]|720p|576p|480p|4k",
        r"|web[ .-]?dl|webrip|blu[ .-]?ray|b[dr]rip|hdtv|dvdrip|hdrip|remux",
        r"|[xh][ .]?26[45]|hevc|xvid|divx|av1",
        r"|aac(?:[ .]?\d[ .]\d)?|ac3|dts|ddp?[ .]?[257][ .]1|flac|10bit)\b",
    ))
});
/// Termini tecnici riconosciuti solo in maiuscolo, perché altrimenti sono parole comuni.
static TECH_UPPER: LazyLock<Regex> =
    LazyLock::new(|| re(r"\b(?:WEB|DVD|UHD|HDR|ITA|ENG|MULTI|PROPER|REPACK)\b"));

static RESOLUTION: LazyLock<Regex> =
    LazyLock::new(|| re(r"(?i)\b(2160p|1080p|1080i|720p|576p|480p|4k)\b"));
static SOURCE: LazyLock<Regex> = LazyLock::new(|| {
    re(r"(?i)\b(web[ .-]?dl|webrip|blu[ .-]?ray|bdrip|brrip|hdtv|dvdrip|hdrip|remux)\b")
});
static SOURCE_UPPER: LazyLock<Regex> = LazyLock::new(|| re(r"\b(WEB|DVD)\b"));

static SXXEXX: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\bS(\d{1,2})[ .-]?E(\d{1,4})"));
static NXNN: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\b(\d{1,2})x(\d{2,3})\b"));
/// Numerazione tipica degli anime: `Titolo - 05`.
static DASH_EPISODE: LazyLock<Regex> = LazyLock::new(|| re(r" - (\d{1,4})(?:v\d)?(?:[ \[(]|$)"));
static SEASON_SUFFIX: LazyLock<Regex> = LazyLock::new(|| re(r"(?i)\bS(\d{1,2})\s*$"));
static TOKEN: LazyLock<Regex> = LazyLock::new(|| re(r"[^ .]+"));
static YEAR: LazyLock<Regex> = LazyLock::new(|| re(r"\b(19\d{2}|20\d{2})\b"));

struct EpisodeMarker {
    /// Posizione in cui termina il titolo.
    start: usize,
    season: Option<u32>,
    episode: u32,
}

/// Analizza il nome di un file (senza cartelle) e ne estrae le informazioni.
pub fn parse(file_name: &str) -> ParsedName {
    let (stem, extension) = split_extension(file_name);
    let mut s = stem.replace('_', " ");

    let leading = LEADING_GROUP
        .captures(&s)
        .map(|c| (c[1].trim().to_string(), c[0].len()));
    let mut group = None;
    if let Some((name, len)) = leading {
        group = Some(name);
        s = s[len..].trim_start().to_string();
    }

    let tech = tech_boundary(&s);
    let marker = find_episode(&s, tech);

    let mut title_end = marker.as_ref().map_or(tech, |m| m.start.min(tech));
    let year_match = YEAR
        .find_iter(&s[..title_end])
        .filter(|m| m.start() > 0)
        .last();
    let year = year_match.and_then(|m| m.as_str().parse().ok());
    if let Some(m) = year_match {
        title_end = m.start();
    }

    if group.is_none() {
        group = trailing_group(&s, tech);
    }

    ParsedName {
        title: clean_title(&s[..title_end]),
        year,
        season: marker.as_ref().and_then(|m| m.season),
        episode: marker.as_ref().map(|m| m.episode),
        quality: quality(&s),
        group,
        extension,
    }
}

fn split_extension(file_name: &str) -> (String, Option<String>) {
    let path = Path::new(file_name);
    let stem = path.file_stem().and_then(|s| s.to_str());
    let ext = path.extension().and_then(|e| e.to_str());
    match (stem, ext) {
        (Some(stem), Some(ext)) if looks_like_extension(ext) => {
            (stem.to_string(), Some(ext.to_ascii_lowercase()))
        }
        _ => (file_name.to_string(), None),
    }
}

fn looks_like_extension(ext: &str) -> bool {
    ext.len() <= 5
        && ext.chars().all(|c| c.is_ascii_alphanumeric())
        && ext.chars().any(|c| c.is_ascii_alphabetic())
        && !RESOLUTION.is_match(ext)
}

/// Posizione del primo termine tecnico, oppure la lunghezza della stringa.
/// Un termine a inizio nome è considerato parte del titolo.
fn tech_boundary(s: &str) -> usize {
    TECH.find_iter(s)
        .chain(TECH_UPPER.find_iter(s))
        .map(|m| m.start())
        .filter(|&start| start > 0)
        .min()
        .unwrap_or(s.len())
}

fn is_year(n: u32) -> bool {
    (1900..=2099).contains(&n)
}

fn find_episode(s: &str, tech: usize) -> Option<EpisodeMarker> {
    for pattern in [&SXXEXX, &NXNN] {
        if let Some(c) = pattern.captures(s) {
            return Some(EpisodeMarker {
                start: c.get(0)?.start(),
                season: c[1].parse().ok(),
                episode: c[2].parse().ok()?,
            });
        }
    }

    let head = &s[..tech];

    if let Some(c) = DASH_EPISODE.captures(head) {
        let episode: u32 = c[1].parse().ok()?;
        if !is_year(episode) {
            let mut start = c.get(0)?.start();
            let mut season = None;
            if let Some(sc) = SEASON_SUFFIX.captures(&head[..start]) {
                season = sc[1].parse().ok();
                start = sc.get(0)?.start();
            }
            return Some(EpisodeMarker {
                start,
                season,
                episode,
            });
        }
    }

    // Forma compatta: 102 = S01E02, 1002 = S10E02.
    for token in TOKEN.find_iter(head) {
        let text = token.as_str();
        if token.start() == 0
            || !(3..=4).contains(&text.len())
            || !text.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        let number: u32 = text.parse().ok()?;
        let (season, episode) = (number / 100, number % 100);
        // Un anno più avanti indica un film con un numero nel titolo.
        let year_follows = YEAR.find_iter(head).any(|m| m.start() >= token.end());
        if is_year(number) || season == 0 || episode == 0 || year_follows {
            continue;
        }
        return Some(EpisodeMarker {
            start: token.start(),
            season: Some(season),
            episode,
        });
    }

    None
}

/// Gruppo in coda al nome (`...x264-GRP`), accettato solo dopo un termine tecnico
/// per non scambiare un titolo con trattino (`Spider-Man`) per un gruppo.
fn trailing_group(s: &str, tech: usize) -> Option<String> {
    let trimmed = TRAILING_BRACKETS.replace(s, "");
    let captures = TRAILING_GROUP.captures(&trimmed)?;
    let name = &captures[1];
    if captures.get(0)?.start() < tech || name.eq_ignore_ascii_case("dl") {
        return None;
    }
    Some(name.to_string())
}

fn quality(s: &str) -> Option<String> {
    let resolution = RESOLUTION.captures(s).map(|c| {
        let value = c[1].to_ascii_lowercase();
        if value == "4k" {
            "2160p".to_string()
        } else {
            value
        }
    });
    let source = SOURCE
        .captures(s)
        .map(|c| canonical_source(&c[1]))
        .or_else(|| SOURCE_UPPER.captures(s).map(|c| c[1].to_string()));

    let parts: Vec<String> = [resolution, source].into_iter().flatten().collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join(" "))
    }
}

fn canonical_source(raw: &str) -> String {
    let key: String = raw
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .collect::<String>()
        .to_ascii_lowercase();
    match key.as_str() {
        "webdl" => "WEB-DL",
        "webrip" => "WEBRip",
        "bluray" => "BluRay",
        "bdrip" => "BDRip",
        "brrip" => "BRRip",
        "hdtv" => "HDTV",
        "dvdrip" => "DVDRip",
        "hdrip" => "HDRip",
        "remux" => "REMUX",
        _ => raw,
    }
    .to_string()
}

fn clean_title(segment: &str) -> String {
    // Con spazi nel nome i punti sono punteggiatura ("Mr. Robot"), altrimenti separatori.
    let spaced = if segment.contains(' ') {
        segment.to_string()
    } else {
        segment.replace('.', " ")
    };
    spaced
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .trim_start_matches([' ', '-', '.'])
        .trim_end_matches([' ', '-', '.', '(', '['])
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// (nome file, titolo, anno, stagione, episodio, qualità, gruppo, estensione)
    type Case = (
        &'static str,
        &'static str,
        Option<u16>,
        Option<u32>,
        Option<u32>,
        Option<&'static str>,
        Option<&'static str>,
        &'static str,
    );

    fn check(cases: &[Case]) {
        for &(name, title, year, season, episode, quality, group, ext) in cases {
            let expected = ParsedName {
                title: title.to_string(),
                year,
                season,
                episode,
                quality: quality.map(str::to_string),
                group: group.map(str::to_string),
                extension: Some(ext.to_string()),
            };
            assert_eq!(parse(name), expected, "file: {name}");
        }
    }

    #[test]
    fn serie_sxxexx() {
        check(&[
            (
                "Show.Name.S01E02.1080p.WEB-DL.x264-GRP.mkv",
                "Show Name",
                None,
                Some(1),
                Some(2),
                Some("1080p WEB-DL"),
                Some("GRP"),
                "mkv",
            ),
            (
                "Mr. Robot - S01E02.MKV",
                "Mr. Robot",
                None,
                Some(1),
                Some(2),
                None,
                None,
                "mkv",
            ),
            (
                "Show.Name.S01E01E02.720p.mkv",
                "Show Name",
                None,
                Some(1),
                Some(1),
                Some("720p"),
                None,
                "mkv",
            ),
            (
                "One.Piece.S21E1085.mkv",
                "One Piece",
                None,
                Some(21),
                Some(1085),
                None,
                None,
                "mkv",
            ),
            (
                "Show_Name_s02e10_HDTV.mp4",
                "Show Name",
                None,
                Some(2),
                Some(10),
                Some("HDTV"),
                None,
                "mp4",
            ),
        ]);
    }

    #[test]
    fn serie_formato_1x02() {
        check(&[
            (
                "show name - 1x02 - Episode Title.avi",
                "show name",
                None,
                Some(1),
                Some(2),
                None,
                None,
                "avi",
            ),
            (
                "La.Casa.di.Carta.3x05.ITA.720p.mkv",
                "La Casa di Carta",
                None,
                Some(3),
                Some(5),
                Some("720p"),
                None,
                "mkv",
            ),
        ]);
    }

    #[test]
    fn serie_formato_compatto() {
        check(&[
            (
                "Show.Name.102.720p.HDTV.mkv",
                "Show Name",
                None,
                Some(1),
                Some(2),
                Some("720p HDTV"),
                None,
                "mkv",
            ),
            (
                "Show.Name.1002.mkv",
                "Show Name",
                None,
                Some(10),
                Some(2),
                None,
                None,
                "mkv",
            ),
        ]);
    }

    #[test]
    fn serie_con_anno_o_numero_nel_titolo() {
        check(&[
            (
                "Doctor.Who.2005.S01E01.Rose.mkv",
                "Doctor Who",
                Some(2005),
                Some(1),
                Some(1),
                None,
                None,
                "mkv",
            ),
            (
                "The.100.S02E05.mkv",
                "The 100",
                None,
                Some(2),
                Some(5),
                None,
                None,
                "mkv",
            ),
            (
                "1923.S01E01.1080p.WEB.h264-GRP.mkv",
                "1923",
                None,
                Some(1),
                Some(1),
                Some("1080p WEB"),
                Some("GRP"),
                "mkv",
            ),
        ]);
    }

    #[test]
    fn film() {
        check(&[
            (
                "Inception.2010.1080p.BluRay.x264-SPARKS.mkv",
                "Inception",
                Some(2010),
                None,
                None,
                Some("1080p BluRay"),
                Some("SPARKS"),
                "mkv",
            ),
            (
                "The Shawshank Redemption (1994).mp4",
                "The Shawshank Redemption",
                Some(1994),
                None,
                None,
                None,
                None,
                "mp4",
            ),
            (
                "Il.Nome.Della.Rosa.1986.iTALiAN.BDRip.XviD-TRL.avi",
                "Il Nome Della Rosa",
                Some(1986),
                None,
                None,
                Some("BDRip"),
                Some("TRL"),
                "avi",
            ),
            (
                "Spider-Man.No.Way.Home.2021.mkv",
                "Spider-Man No Way Home",
                Some(2021),
                None,
                None,
                None,
                None,
                "mkv",
            ),
        ]);
    }

    #[test]
    fn film_con_anno_o_numero_nel_titolo() {
        check(&[
            (
                "Blade.Runner.2049.2017.2160p.UHD.BluRay.x265-GRP.mkv",
                "Blade Runner 2049",
                Some(2017),
                None,
                None,
                Some("2160p BluRay"),
                Some("GRP"),
                "mkv",
            ),
            (
                "2001.A.Space.Odyssey.1968.720p.mkv",
                "2001 A Space Odyssey",
                Some(1968),
                None,
                None,
                Some("720p"),
                None,
                "mkv",
            ),
            (
                "2012.2009.BDRip.XviD.avi",
                "2012",
                Some(2009),
                None,
                None,
                Some("BDRip"),
                None,
                "avi",
            ),
            (
                "Fahrenheit.451.1966.DVDRip.avi",
                "Fahrenheit 451",
                Some(1966),
                None,
                None,
                Some("DVDRip"),
                None,
                "avi",
            ),
            (
                "James.Bond.007.mkv",
                "James Bond 007",
                None,
                None,
                None,
                None,
                None,
                "mkv",
            ),
        ]);
    }

    #[test]
    fn anime() {
        check(&[
            (
                "[SubsPlease] Sousou no Frieren - 05 (1080p) [ABCD1234].mkv",
                "Sousou no Frieren",
                None,
                None,
                Some(5),
                Some("1080p"),
                Some("SubsPlease"),
                "mkv",
            ),
            (
                "[Erai-raws] Spy x Family S2 - 03 [720p][Multiple Subtitle].mkv",
                "Spy x Family",
                None,
                Some(2),
                Some(3),
                Some("720p"),
                Some("Erai-raws"),
                "mkv",
            ),
            (
                "[Group]_Anime_Title_-_12v2_[BD_1080p].mkv",
                "Anime Title",
                None,
                None,
                Some(12),
                Some("1080p"),
                Some("Group"),
                "mkv",
            ),
        ]);
    }

    #[test]
    fn gruppo_non_confuso_con_web_dl() {
        let parsed = parse("Show.Name.S01E02.1080p.WEB-DL.mkv");
        assert_eq!(parsed.group, None);
        assert_eq!(parsed.quality.as_deref(), Some("1080p WEB-DL"));
    }

    #[test]
    fn tipo_dedotto_dal_numero_di_episodio() {
        assert_eq!(parse("Show.Name.S01E02.mkv").kind(), MediaKind::Episode);
        assert_eq!(parse("Inception.2010.mkv").kind(), MediaKind::Movie);
    }

    #[test]
    fn nome_senza_estensione() {
        let parsed = parse("Show.Name.S01E02");
        assert_eq!(parsed.title, "Show Name");
        assert_eq!(parsed.extension, None);
    }
}
