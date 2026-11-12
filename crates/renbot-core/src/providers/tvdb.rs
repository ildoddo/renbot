//! TheTVDB (API v4): serie e anime.

use std::collections::HashMap;
use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::json;
use tokio::sync::Mutex;

use super::{
    decode, distinct_original, year_of, Episode, MetadataProvider, ProviderError, ProviderId,
    ProviderResult, SearchResult,
};
use crate::http::{BoxFuture, Fetcher, HttpError, HttpRequest};
use crate::parser::MediaKind;

const BASE_URL: &str = "https://api4.thetvdb.com/v4";
/// Limite di sicurezza sulle pagine dell'elenco episodi (500 episodi per pagina).
const MAX_PAGES: u32 = 50;

pub struct Tvdb {
    fetcher: Arc<Fetcher>,
    api_key: String,
    /// Token di sessione ottenuto dal login, valido per più richieste.
    token: Mutex<Option<String>>,
}

#[derive(Deserialize)]
struct LoginEnvelope {
    data: Login,
}

#[derive(Deserialize)]
struct Login {
    token: String,
}

#[derive(Deserialize)]
struct SearchEnvelope {
    data: Option<Vec<SearchItem>>,
}

#[derive(Deserialize)]
struct SearchItem {
    tvdb_id: Option<String>,
    name: Option<String>,
    year: Option<String>,
    translations: Option<HashMap<String, String>>,
}

#[derive(Deserialize)]
struct EpisodesEnvelope {
    data: EpisodesData,
    links: Option<Links>,
}

#[derive(Deserialize)]
struct EpisodesData {
    #[serde(default)]
    episodes: Vec<EpisodeItem>,
}

#[derive(Deserialize)]
struct Links {
    next: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct EpisodeItem {
    season_number: Option<u32>,
    number: Option<u32>,
    name: Option<String>,
    aired: Option<String>,
}

impl Tvdb {
    pub fn new(fetcher: Arc<Fetcher>, api_key: impl Into<String>) -> Self {
        Self {
            fetcher,
            api_key: api_key.into(),
            token: Mutex::new(None),
        }
    }

    async fn token(&self) -> ProviderResult<String> {
        let mut token = self.token.lock().await;
        if let Some(token) = token.as_ref() {
            return Ok(token.clone());
        }
        let request =
            HttpRequest::post(format!("{BASE_URL}/login"), json!({"apikey": self.api_key}));
        let login: LoginEnvelope = decode(self.fetcher.json(&request, false).await?)?;
        *token = Some(login.data.token.clone());
        Ok(login.data.token)
    }

    /// GET autenticata. Se il token è scaduto ripete il login una volta.
    async fn get<T: DeserializeOwned>(
        &self,
        path: &str,
        query: &[(&str, String)],
    ) -> ProviderResult<T> {
        let mut renewed = false;
        loop {
            let mut request =
                HttpRequest::get(format!("{BASE_URL}{path}")).bearer(self.token().await?);
            for (name, value) in query {
                request = request.query(name, value.as_str());
            }
            match self.fetcher.json(&request, true).await {
                Err(HttpError::Unauthorized) if !renewed => {
                    renewed = true;
                    *self.token.lock().await = None;
                }
                result => return decode(result?),
            }
        }
    }
}

/// TheTVDB usa codici a tre lettere (ISO 639-2).
fn language_code(language: &str) -> &'static str {
    match language {
        "it" => "ita",
        "de" => "deu",
        "fr" => "fra",
        "es" => "spa",
        "pt" => "por",
        "ja" => "jpn",
        _ => "eng",
    }
}

impl MetadataProvider for Tvdb {
    fn id(&self) -> ProviderId {
        ProviderId::Tvdb
    }

    /// TheTVDB è usato solo per serie e anime: per i film non restituisce risultati.
    fn search<'a>(
        &'a self,
        kind: MediaKind,
        query: &'a str,
        year: Option<u16>,
        language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<SearchResult>>> {
        Box::pin(async move {
            if kind == MediaKind::Movie {
                return Ok(Vec::new());
            }
            let mut parameters = vec![("query", query.to_string()), ("type", "series".to_string())];
            if let Some(year) = year {
                parameters.push(("year", year.to_string()));
            }
            let found: SearchEnvelope = self.get("/search", &parameters).await?;

            let code = language_code(language);
            Ok(found
                .data
                .unwrap_or_default()
                .into_iter()
                .filter_map(|item| {
                    let name = item.name.unwrap_or_default();
                    let translated = item.translations.and_then(|mut t| t.remove(code));
                    let title = translated.filter(|t| !t.is_empty()).unwrap_or(name.clone());
                    Some(SearchResult {
                        provider: ProviderId::Tvdb,
                        id: item.tvdb_id?,
                        kind: MediaKind::Episode,
                        original_title: distinct_original(&title, Some(name)),
                        title,
                        year: year_of(item.year.as_deref()),
                    })
                })
                .collect())
        })
    }

    fn episodes<'a>(
        &'a self,
        series_id: &'a str,
        language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<Episode>>> {
        Box::pin(async move {
            let path = format!(
                "/series/{series_id}/episodes/default/{}",
                language_code(language)
            );
            let mut episodes = Vec::new();
            for page in 0..MAX_PAGES {
                let response: EpisodesEnvelope =
                    self.get(&path, &[("page", page.to_string())]).await?;
                episodes.extend(response.data.episodes.into_iter().filter_map(|e| {
                    Some(Episode {
                        season: e.season_number?,
                        number: e.number?,
                        title: e.name.unwrap_or_default(),
                        air_date: e.aired.filter(|d| !d.is_empty()),
                    })
                }));
                if response.links.and_then(|l| l.next).is_none() {
                    break;
                }
            }
            episodes.sort_by_key(|e| (e.season, e.number));
            Ok(episodes)
        })
    }

    /// La ricerca per ID IMDb passa da TMDB.
    fn find_by_imdb_id<'a>(
        &'a self,
        _imdb_id: &'a str,
        _language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<SearchResult>>> {
        Box::pin(async { Err(ProviderError::Unsupported) })
    }

    fn verify_key(&self) -> BoxFuture<'_, ProviderResult<()>> {
        Box::pin(async move {
            *self.token.lock().await = None;
            self.token().await.map(|_| ())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{fetcher, ok, status, FakeClient};

    const KEY: &str = "chiave-di-prova";

    fn tvdb(client: &Arc<FakeClient>) -> Tvdb {
        Tvdb::new(fetcher(client.clone()), KEY)
    }

    fn login(client: &FakeClient, token: &str) {
        client.route_json(
            "/login",
            json!({"status": "success", "data": {"token": token}}),
        );
    }

    #[tokio::test]
    async fn accede_una_volta_e_cerca_una_serie() {
        let client = FakeClient::new();
        login(&client, "TOKEN");
        client.route_json(
            "/search",
            json!({"status": "success", "data": [
                {"objectID": "series-327417", "tvdb_id": "327417", "type": "series",
                 "name": "La casa de papel", "year": "2017",
                 "translations": {"eng": "Money Heist", "ita": "La casa di carta"}},
                {"objectID": "series-1", "name": "Senza identificativo", "year": "2000"}
            ]}),
        );
        let provider = tvdb(&client);
        let found = provider
            .search(MediaKind::Episode, "casa di carta", Some(2017), "it")
            .await
            .unwrap();
        provider
            .search(MediaKind::Episode, "casa di carta", Some(2017), "it")
            .await
            .unwrap();

        assert_eq!(
            found,
            [SearchResult {
                provider: ProviderId::Tvdb,
                id: "327417".to_string(),
                kind: MediaKind::Episode,
                title: "La casa di carta".to_string(),
                original_title: Some("La casa de papel".to_string()),
                year: Some(2017),
            }]
        );
        assert_eq!(client.count("/login"), 1);

        let requests = client.requests();
        assert_eq!(requests[0].json_body, Some(json!({"apikey": KEY})));
        assert_eq!(
            requests[1].describe(),
            "GET https://api4.thetvdb.com/v4/search?query=casa di carta&type=series&year=2017"
        );
        assert_eq!(requests[1].bearer.as_deref(), Some("TOKEN"));
    }

    #[tokio::test]
    async fn per_i_film_non_interroga_il_server() {
        let client = FakeClient::new();
        let found = tvdb(&client)
            .search(MediaKind::Movie, "Inception", None, "it")
            .await
            .unwrap();
        assert!(found.is_empty());
        assert!(client.requests().is_empty());
    }

    #[tokio::test]
    async fn elenca_gli_episodi_su_più_pagine() {
        let client = FakeClient::new();
        login(&client, "TOKEN");
        client.route_json(
            "page=0",
            json!({"data": {"episodes": [
                {"id": 2, "seasonNumber": 1, "number": 2, "name": "Secondo", "aired": "2020-01-08"},
                {"id": 1, "seasonNumber": 1, "number": 1, "name": "Primo", "aired": "2020-01-01"}
            ]}, "links": {"next": "https://api4.thetvdb.com/v4/series/5/episodes/default/ita?page=1"}}),
        );
        client.route_json(
            "page=1",
            json!({"data": {"episodes": [
                {"id": 3, "seasonNumber": 2, "number": 1, "name": null, "aired": null}
            ]}, "links": {"next": null}}),
        );
        let episodes = tvdb(&client).episodes("5", "it").await.unwrap();
        let summary: Vec<(u32, u32, &str)> = episodes
            .iter()
            .map(|e| (e.season, e.number, e.title.as_str()))
            .collect();
        assert_eq!(summary, [(1, 1, "Primo"), (1, 2, "Secondo"), (2, 1, "")]);
        assert_eq!(client.count("/series/5/episodes/default/ita"), 2);
    }

    #[tokio::test]
    async fn rinnova_il_token_scaduto() {
        let client = FakeClient::new();
        client.route(
            "/login",
            vec![
                ok(json!({"data": {"token": "VECCHIO"}})),
                ok(json!({"data": {"token": "NUOVO"}})),
            ],
        );
        client.route("/search", vec![status(401), ok(json!({"data": []}))]);
        let found = tvdb(&client)
            .search(MediaKind::Episode, "serie", None, "it")
            .await
            .unwrap();
        assert!(found.is_empty());
        assert_eq!(client.count("/login"), 2);
        let bearers: Vec<Option<String>> = client
            .requests()
            .into_iter()
            .filter(|r| r.describe().contains("/search"))
            .map(|r| r.bearer)
            .collect();
        assert_eq!(
            bearers,
            [Some("VECCHIO".to_string()), Some("NUOVO".to_string())]
        );
    }

    #[tokio::test]
    async fn verifica_della_chiave() {
        let client = FakeClient::new();
        login(&client, "TOKEN");
        assert_eq!(tvdb(&client).verify_key().await, Ok(()));

        let client = FakeClient::new();
        client.route("/login", vec![status(401)]);
        assert_eq!(
            tvdb(&client).verify_key().await,
            Err(ProviderError::Http(HttpError::Unauthorized))
        );
    }
}
