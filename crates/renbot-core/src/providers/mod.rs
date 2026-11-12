//! Database di metadati: tipi comuni, trait e ricerca con database di riserva.

pub mod omdb;
pub mod tmdb;
pub mod tvdb;

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::http::{BoxFuture, HttpError};
use crate::parser::MediaKind;

/// Lingua predefinita per titoli ed episodi.
pub const DEFAULT_LANGUAGE: &str = "it";
/// Lingua usata per completare i titoli mancanti nella lingua richiesta.
pub const FALLBACK_LANGUAGE: &str = "en";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ProviderId {
    Tmdb,
    Tvdb,
}

/// Un film o una serie trovati da una ricerca.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchResult {
    pub provider: ProviderId,
    /// Identificativo presso il database di origine.
    pub id: String,
    pub kind: MediaKind,
    pub title: String,
    /// Titolo originale, se diverso da `title`.
    pub original_title: Option<String>,
    pub year: Option<u16>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Episode {
    pub season: u32,
    pub number: u32,
    pub title: String,
    /// Data di prima messa in onda, nel formato `AAAA-MM-GG`.
    pub air_date: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProviderError {
    #[error(transparent)]
    Http(#[from] HttpError),
    #[error("formato della risposta inatteso: {0}")]
    UnexpectedResponse(String),
    #[error("operazione non supportata da questo database")]
    Unsupported,
}

type ProviderResult<T> = Result<T, ProviderError>;

/// Operazioni comuni ai database. `language` è un codice ISO 639-1, ad esempio `it`.
pub trait MetadataProvider: Send + Sync {
    fn id(&self) -> ProviderId;

    /// Cerca film o serie per titolo, con anno facoltativo.
    fn search<'a>(
        &'a self,
        kind: MediaKind,
        query: &'a str,
        year: Option<u16>,
        language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<SearchResult>>>;

    /// Tutti gli episodi di una serie, ordinati per stagione e numero.
    fn episodes<'a>(
        &'a self,
        series_id: &'a str,
        language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<Episode>>>;

    /// Cerca per identificativo IMDb, ad esempio `tt0111161`.
    fn find_by_imdb_id<'a>(
        &'a self,
        imdb_id: &'a str,
        language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<SearchResult>>>;

    /// Verifica la chiave API con una chiamata reale, senza passare dalla cache.
    fn verify_key(&self) -> BoxFuture<'_, ProviderResult<()>>;
}

/// Cerca sul database principale e, se non trova nulla, su quello di riserva.
/// Restituisce i risultati insieme al database che li ha forniti.
pub async fn search(
    primary: &dyn MetadataProvider,
    fallback: Option<&dyn MetadataProvider>,
    kind: MediaKind,
    query: &str,
    year: Option<u16>,
    language: &str,
) -> ProviderResult<(ProviderId, Vec<SearchResult>)> {
    let found = search_one(primary, kind, query, year, language).await?;
    match fallback {
        Some(other) if found.is_empty() => {
            tracing::info!(from = ?primary.id(), to = ?other.id(), "nessun risultato, uso l'altro database");
            let found = search_one(other, kind, query, year, language).await?;
            Ok((other.id(), found))
        }
        _ => Ok((primary.id(), found)),
    }
}

/// Ricerca su un solo database. L'anno ricavato dal nome del file può essere
/// sbagliato, quindi una ricerca senza risultati viene ripetuta senza anno.
async fn search_one(
    provider: &dyn MetadataProvider,
    kind: MediaKind,
    query: &str,
    year: Option<u16>,
    language: &str,
) -> ProviderResult<Vec<SearchResult>> {
    let found = provider.search(kind, query, year, language).await?;
    if found.is_empty() && year.is_some() {
        return provider.search(kind, query, None, language).await;
    }
    Ok(found)
}

/// Episodi nella lingua richiesta; i titoli mancanti sono completati in inglese.
pub async fn episodes_localized(
    provider: &dyn MetadataProvider,
    series_id: &str,
    language: &str,
) -> ProviderResult<Vec<Episode>> {
    let mut episodes = provider.episodes(series_id, language).await?;
    let incomplete = episodes.iter().any(|e| e.title.trim().is_empty());
    if incomplete && language != FALLBACK_LANGUAGE {
        let english = provider.episodes(series_id, FALLBACK_LANGUAGE).await?;
        for episode in episodes.iter_mut().filter(|e| e.title.trim().is_empty()) {
            if let Some(source) = english
                .iter()
                .find(|e| e.season == episode.season && e.number == episode.number)
            {
                episode.title = source.title.clone();
            }
        }
    }
    Ok(episodes)
}

fn decode<T: DeserializeOwned>(value: Value) -> ProviderResult<T> {
    serde_json::from_value(value).map_err(|e| ProviderError::UnexpectedResponse(e.to_string()))
}

/// Anno iniziale di una data `AAAA-MM-GG` o di una stringa `AAAA`.
fn year_of(date: Option<&str>) -> Option<u16> {
    date?.get(..4)?.parse().ok()
}

/// Mantiene il titolo originale solo se aggiunge informazione.
fn distinct_original(title: &str, original: Option<String>) -> Option<String> {
    original.filter(|o| !o.is_empty() && o != title)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use super::*;

    /// Database simulato: risponde con risultati fissi e registra le chiamate.
    struct FakeProvider {
        id: ProviderId,
        /// Risultati restituiti solo quando la ricerca non filtra per anno.
        only_without_year: bool,
        results: Vec<SearchResult>,
        localized: Vec<Episode>,
        english: Vec<Episode>,
        calls: Mutex<Vec<Option<u16>>>,
    }

    impl FakeProvider {
        fn new(id: ProviderId, titles: &[&str]) -> Self {
            let results = titles
                .iter()
                .enumerate()
                .map(|(i, title)| SearchResult {
                    provider: id,
                    id: i.to_string(),
                    kind: MediaKind::Episode,
                    title: title.to_string(),
                    original_title: None,
                    year: None,
                })
                .collect();
            Self {
                id,
                only_without_year: false,
                results,
                localized: Vec::new(),
                english: Vec::new(),
                calls: Mutex::new(Vec::new()),
            }
        }

        fn calls(&self) -> Vec<Option<u16>> {
            self.calls.lock().unwrap().clone()
        }
    }

    impl MetadataProvider for FakeProvider {
        fn id(&self) -> ProviderId {
            self.id
        }

        fn search<'a>(
            &'a self,
            _kind: MediaKind,
            _query: &'a str,
            year: Option<u16>,
            _language: &'a str,
        ) -> BoxFuture<'a, ProviderResult<Vec<SearchResult>>> {
            self.calls.lock().unwrap().push(year);
            let hidden = self.only_without_year && year.is_some();
            let results = if hidden {
                Vec::new()
            } else {
                self.results.clone()
            };
            Box::pin(async move { Ok(results) })
        }

        fn episodes<'a>(
            &'a self,
            _series_id: &'a str,
            language: &'a str,
        ) -> BoxFuture<'a, ProviderResult<Vec<Episode>>> {
            let episodes = if language == FALLBACK_LANGUAGE {
                self.english.clone()
            } else {
                self.localized.clone()
            };
            Box::pin(async move { Ok(episodes) })
        }

        fn find_by_imdb_id<'a>(
            &'a self,
            _imdb_id: &'a str,
            _language: &'a str,
        ) -> BoxFuture<'a, ProviderResult<Vec<SearchResult>>> {
            Box::pin(async { Err(ProviderError::Unsupported) })
        }

        fn verify_key(&self) -> BoxFuture<'_, ProviderResult<()>> {
            Box::pin(async { Ok(()) })
        }
    }

    async fn run(
        primary: &FakeProvider,
        fallback: Option<&FakeProvider>,
        year: Option<u16>,
    ) -> (ProviderId, Vec<String>) {
        let fallback = fallback.map(|f| f as &dyn MetadataProvider);
        let (id, found) = search(primary, fallback, MediaKind::Episode, "serie", year, "it")
            .await
            .unwrap();
        (id, found.into_iter().map(|r| r.title).collect())
    }

    #[tokio::test]
    async fn fallback_da_tmdb_a_tvdb() {
        let tmdb = FakeProvider::new(ProviderId::Tmdb, &[]);
        let tvdb = FakeProvider::new(ProviderId::Tvdb, &["Serie su TVDB"]);
        let (id, titles) = run(&tmdb, Some(&tvdb), None).await;
        assert_eq!(id, ProviderId::Tvdb);
        assert_eq!(titles, ["Serie su TVDB"]);
    }

    #[tokio::test]
    async fn fallback_da_tvdb_a_tmdb() {
        let tvdb = FakeProvider::new(ProviderId::Tvdb, &[]);
        let tmdb = FakeProvider::new(ProviderId::Tmdb, &["Serie su TMDB"]);
        let (id, titles) = run(&tvdb, Some(&tmdb), None).await;
        assert_eq!(id, ProviderId::Tmdb);
        assert_eq!(titles, ["Serie su TMDB"]);
    }

    #[tokio::test]
    async fn il_database_principale_ha_la_precedenza() {
        let tmdb = FakeProvider::new(ProviderId::Tmdb, &["Serie su TMDB"]);
        let tvdb = FakeProvider::new(ProviderId::Tvdb, &["Serie su TVDB"]);
        let (id, titles) = run(&tmdb, Some(&tvdb), None).await;
        assert_eq!(id, ProviderId::Tmdb);
        assert_eq!(titles, ["Serie su TMDB"]);
        assert!(tvdb.calls().is_empty());
    }

    #[tokio::test]
    async fn senza_fallback_non_interroga_l_altro_database() {
        let tmdb = FakeProvider::new(ProviderId::Tmdb, &[]);
        let (id, titles) = run(&tmdb, None, None).await;
        assert_eq!(id, ProviderId::Tmdb);
        assert!(titles.is_empty());
    }

    #[tokio::test]
    async fn ripete_la_ricerca_senza_anno() {
        let mut tmdb = FakeProvider::new(ProviderId::Tmdb, &["Blade Runner 2049"]);
        tmdb.only_without_year = true;
        let (_, titles) = run(&tmdb, None, Some(2049)).await;
        assert_eq!(titles, ["Blade Runner 2049"]);
        assert_eq!(tmdb.calls(), [Some(2049), None]);
    }

    fn episode(number: u32, title: &str) -> Episode {
        Episode {
            season: 1,
            number,
            title: title.to_string(),
            air_date: None,
        }
    }

    #[tokio::test]
    async fn completa_in_inglese_i_titoli_mancanti() {
        let mut provider = FakeProvider::new(ProviderId::Tmdb, &[]);
        provider.localized = vec![episode(1, "Episodio pilota"), episode(2, "")];
        provider.english = vec![episode(1, "Pilot"), episode(2, "Second")];
        let episodes = episodes_localized(&provider, "1", "it").await.unwrap();
        let titles: Vec<&str> = episodes.iter().map(|e| e.title.as_str()).collect();
        assert_eq!(titles, ["Episodio pilota", "Second"]);
    }
}
