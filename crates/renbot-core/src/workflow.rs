//! Abbinamento automatico di un file: i passi comuni a interfaccia grafica e riga di comando.

use std::path::PathBuf;

use crate::matcher::{self, ScoredResult};
use crate::parser::{MediaKind, ParsedName};
use crate::providers::{self, Episode, MetadataProvider, ProviderError, SearchResult};
use crate::renamer::format::{render, FormatValues};

/// Esito dell'abbinamento di un file.
#[derive(Debug, Clone, PartialEq)]
pub struct Match {
    /// Candidati ordinati per punteggio; vuoto se la ricerca non ha trovato nulla.
    pub candidates: Vec<ScoredResult>,
    /// Episodi del primo candidato, se è una serie.
    pub episodes: Vec<Episode>,
}

/// Cerca il titolo ricavato dal nome del file, ordina i candidati e, per una serie,
/// scarica gli episodi del migliore dal database che lo ha trovato.
pub async fn find_match(
    primary: &dyn MetadataProvider,
    fallback: Option<&dyn MetadataProvider>,
    parsed: &ParsedName,
    language: &str,
) -> Result<Match, ProviderError> {
    let (source, found) = providers::search(
        primary,
        fallback,
        parsed.kind(),
        &parsed.title,
        parsed.year,
        language,
    )
    .await?;
    let candidates = matcher::rank(&parsed.title, parsed.year, found);

    let mut episodes = Vec::new();
    let series = candidates
        .first()
        .filter(|best| best.result.kind == MediaKind::Episode);
    if let Some(best) = series {
        let provider = match fallback {
            Some(fallback) if fallback.id() == source => fallback,
            _ => primary,
        };
        episodes = providers::episodes_localized(provider, &best.result.id, language).await?;
    }
    Ok(Match {
        candidates,
        episodes,
    })
}

/// Percorso relativo del nuovo nome per un film o per l'episodio indicato dal nome del file.
/// `None` se la serie non contiene quell'episodio.
pub fn relative_target(
    parsed: &ParsedName,
    result: &SearchResult,
    episodes: &[Episode],
    movie_format: &str,
    series_format: &str,
) -> Option<PathBuf> {
    match result.kind {
        MediaKind::Movie => Some(render(movie_format, &FormatValues::movie(parsed, result))),
        MediaKind::Episode => {
            // Un nome senza stagione (tipico degli anime) è cercato nella prima.
            let season = parsed.season.unwrap_or(1);
            let episode = episodes
                .iter()
                .find(|e| e.season == season && Some(e.number) == parsed.episode)?;
            Some(render(
                series_format,
                &FormatValues::episode(parsed, result, episode),
            ))
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::parser;
    use crate::providers::ProviderId;
    use crate::renamer::format::{DEFAULT_MOVIE_FORMAT, DEFAULT_SERIES_FORMAT};
    use crate::testing::FakeProvider;

    fn target(name: &str, found: &Match) -> Option<PathBuf> {
        relative_target(
            &parser::parse(name),
            &found.candidates.first()?.result,
            &found.episodes,
            DEFAULT_MOVIE_FORMAT,
            DEFAULT_SERIES_FORMAT,
        )
    }

    #[tokio::test]
    async fn abbina_un_episodio_e_un_film() {
        let provider = FakeProvider::new(ProviderId::Tmdb)
            .with_result(MediaKind::Episode, "1", "Show Name", 2019)
            .with_result(MediaKind::Movie, "9", "Inception", 2010)
            .with_episode(1, 2, "Secondo");

        let name = "Show.Name.S01E02.mkv";
        let found = find_match(&provider, None, &parser::parse(name), "it")
            .await
            .unwrap();
        assert_eq!(found.candidates[0].result.id, "1");
        assert_eq!(
            target(name, &found).unwrap(),
            Path::new("Show Name")
                .join("Season 1")
                .join("Show Name - S01E02 - Secondo.mkv")
        );
        assert_eq!(target("Show.Name.S01E09.mkv", &found), None);

        let name = "Inception.2010.1080p.mkv";
        let found = find_match(&provider, None, &parser::parse(name), "it")
            .await
            .unwrap();
        assert!(found.episodes.is_empty());
        assert_eq!(
            target(name, &found).unwrap(),
            Path::new("Inception (2010)").join("Inception (2010).mkv")
        );
    }

    #[tokio::test]
    async fn gli_episodi_arrivano_dal_database_che_ha_trovato_la_serie() {
        let empty = FakeProvider::new(ProviderId::Tmdb).with_episode(1, 1, "Sbagliato");
        let other = FakeProvider::new(ProviderId::Tvdb)
            .with_result(MediaKind::Episode, "7", "Show Name", 2019)
            .with_episode(1, 1, "Giusto");
        let parsed = parser::parse("Show.Name.S01E01.mkv");
        let found = find_match(&empty, Some(&other), &parsed, "it")
            .await
            .unwrap();
        assert_eq!(found.candidates[0].result.provider, ProviderId::Tvdb);
        assert_eq!(found.episodes[0].title, "Giusto");
    }

    #[tokio::test]
    async fn nessun_risultato() {
        let provider = FakeProvider::new(ProviderId::Tmdb);
        let parsed = parser::parse("Sconosciuto.S01E01.mkv");
        let found = find_match(&provider, None, &parsed, "it").await.unwrap();
        assert!(found.candidates.is_empty());
    }
}
