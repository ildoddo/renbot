//! Confronto approssimato tra il titolo ricavato dal file e i risultati di una ricerca.

use crate::providers::SearchResult;

/// Sotto questo punteggio l'abbinamento va mostrato come "da verificare".
pub const REVIEW_THRESHOLD: f64 = 0.75;

#[derive(Debug, Clone, PartialEq)]
pub struct ScoredResult {
    pub result: SearchResult,
    /// Da 0 (nessuna somiglianza) a 1 (titolo e anno coincidenti).
    pub score: f64,
}

impl ScoredResult {
    pub fn needs_review(&self) -> bool {
        self.score < REVIEW_THRESHOLD
    }
}

/// Ordina i candidati dal più al meno somigliante. A parità di punteggio
/// resta l'ordine del database, che riflette la popolarità.
pub fn rank(title: &str, year: Option<u16>, candidates: Vec<SearchResult>) -> Vec<ScoredResult> {
    let mut scored: Vec<ScoredResult> = candidates
        .into_iter()
        .map(|result| ScoredResult {
            score: score(title, year, &result),
            result,
        })
        .collect();
    scored.sort_by(|a, b| b.score.total_cmp(&a.score));
    scored
}

/// Somiglianza del titolo (anche con quello originale), corretta in base all'anno.
pub fn score(title: &str, year: Option<u16>, candidate: &SearchResult) -> f64 {
    let wanted = normalize(title);
    let similarity = std::iter::once(candidate.title.as_str())
        .chain(candidate.original_title.as_deref())
        .map(|name| strsim::normalized_levenshtein(&wanted, &normalize(name)))
        .fold(0.0, f64::max);

    let adjustment = match (year, candidate.year) {
        (Some(wanted), Some(actual)) => match wanted.abs_diff(actual) {
            0 => 0.10,
            1 => 0.03,
            _ => -0.20,
        },
        _ => 0.0,
    };
    (similarity + adjustment).clamp(0.0, 1.0)
}

/// Minuscole, senza punteggiatura e con spazi singoli.
fn normalize(text: &str) -> String {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();
    cleaned
        .to_lowercase()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::MediaKind;
    use crate::providers::ProviderId;

    fn candidate(title: &str, original: Option<&str>, year: Option<u16>) -> SearchResult {
        SearchResult {
            provider: ProviderId::Tmdb,
            id: title.to_string(),
            kind: MediaKind::Movie,
            title: title.to_string(),
            original_title: original.map(str::to_string),
            year,
        }
    }

    fn titles(ranked: &[ScoredResult]) -> Vec<&str> {
        ranked.iter().map(|s| s.result.title.as_str()).collect()
    }

    #[test]
    fn ordina_per_somiglianza_del_titolo() {
        let ranked = rank(
            "Inception",
            None,
            vec![
                candidate("Inception: The Cobol Job", None, Some(2010)),
                candidate("Inception", None, Some(2010)),
                candidate("Interstellar", None, Some(2014)),
            ],
        );
        assert_eq!(
            titles(&ranked),
            ["Inception", "Inception: The Cobol Job", "Interstellar"]
        );
        assert_eq!(ranked[0].score, 1.0);
        assert!(!ranked[0].needs_review());
        assert!(ranked[2].needs_review());
    }

    #[test]
    fn l_anno_distingue_i_titoli_uguali() {
        let ranked = rank(
            "Dune",
            Some(2021),
            vec![
                candidate("Dune", None, Some(1984)),
                candidate("Dune", None, Some(2021)),
            ],
        );
        assert_eq!(ranked[0].result.year, Some(2021));
        assert!(ranked[0].score > ranked[1].score);
    }

    #[test]
    fn ignora_punteggiatura_e_maiuscole() {
        let wanted = "spider man no way home";
        let exact = candidate("Spider-Man: No Way Home", None, None);
        assert_eq!(score(wanted, None, &exact), 1.0);
        assert_eq!(
            score("S W A T", None, &candidate("S.W.A.T.", None, None)),
            1.0
        );
    }

    #[test]
    fn considera_il_titolo_originale() {
        let result = candidate("La casa di carta", Some("La casa de papel"), Some(2017));
        assert_eq!(score("La Casa de Papel", Some(2017), &result), 1.0);
    }

    #[test]
    fn a_parità_di_punteggio_mantiene_l_ordine_del_database() {
        let ranked = rank(
            "Serie",
            None,
            vec![
                candidate("Serie", None, Some(2001)),
                candidate("Serie", None, Some(1990)),
            ],
        );
        assert_eq!(ranked[0].result.year, Some(2001));
    }

    #[test]
    fn titolo_sbagliato_da_verificare() {
        let ranked = rank(
            "Show Nam",
            Some(2019),
            vec![candidate("Show Name", None, Some(2015))],
        );
        assert!(ranked[0].needs_review());
    }
}
