//! TheMovieDB (API v3): film, serie e anime.

use std::sync::Arc;

use serde::de::DeserializeOwned;
use serde::Deserialize;

use super::{
    decode, distinct_original, year_of, Episode, MetadataProvider, ProviderId, ProviderResult,
    SearchResult,
};
use crate::http::{BoxFuture, Fetcher, HttpRequest};
use crate::parser::MediaKind;

const BASE_URL: &str = "https://api.themoviedb.org/3";

pub struct Tmdb {
    fetcher: Arc<Fetcher>,
    api_key: String,
}

#[derive(Deserialize)]
struct Page<T> {
    results: Vec<T>,
}

#[derive(Deserialize)]
struct MovieItem {
    id: u64,
    title: Option<String>,
    original_title: Option<String>,
    release_date: Option<String>,
}

#[derive(Deserialize)]
struct TvItem {
    id: u64,
    name: Option<String>,
    original_name: Option<String>,
    first_air_date: Option<String>,
}

#[derive(Deserialize)]
struct TvDetails {
    #[serde(default)]
    seasons: Vec<SeasonRef>,
}

#[derive(Deserialize)]
struct SeasonRef {
    season_number: u32,
}

#[derive(Deserialize)]
struct Season {
    #[serde(default)]
    episodes: Vec<EpisodeItem>,
}

#[derive(Deserialize)]
struct EpisodeItem {
    season_number: u32,
    episode_number: u32,
    name: Option<String>,
    air_date: Option<String>,
}

#[derive(Deserialize)]
struct FindResponse {
    #[serde(default)]
    movie_results: Vec<MovieItem>,
    #[serde(default)]
    tv_results: Vec<TvItem>,
}

impl From<MovieItem> for SearchResult {
    fn from(item: MovieItem) -> Self {
        let title = item.title.unwrap_or_default();
        Self {
            provider: ProviderId::Tmdb,
            id: item.id.to_string(),
            kind: MediaKind::Movie,
            original_title: distinct_original(&title, item.original_title),
            title,
            year: year_of(item.release_date.as_deref()),
        }
    }
}

impl From<TvItem> for SearchResult {
    fn from(item: TvItem) -> Self {
        let title = item.name.unwrap_or_default();
        Self {
            provider: ProviderId::Tmdb,
            id: item.id.to_string(),
            kind: MediaKind::Episode,
            original_title: distinct_original(&title, item.original_name),
            title,
            year: year_of(item.first_air_date.as_deref()),
        }
    }
}

impl Tmdb {
    pub fn new(fetcher: Arc<Fetcher>, api_key: impl Into<String>) -> Self {
        Self {
            fetcher,
            api_key: api_key.into(),
        }
    }

    /// TMDB accetta sia la chiave API v3 (parametro) sia il token di lettura v4 (intestazione).
    fn request(&self, path: &str) -> HttpRequest {
        let request = HttpRequest::get(format!("{BASE_URL}{path}"));
        if is_read_access_token(&self.api_key) {
            request.bearer(&self.api_key)
        } else {
            request.secret_query("api_key", &self.api_key)
        }
    }

    async fn get<T: DeserializeOwned>(
        &self,
        request: HttpRequest,
        use_cache: bool,
    ) -> ProviderResult<T> {
        decode(self.fetcher.json(&request, use_cache).await?)
    }
}

/// Il token v4 è un JWT: tre parti separate da punti.
fn is_read_access_token(key: &str) -> bool {
    key.matches('.').count() == 2
}

impl MetadataProvider for Tmdb {
    fn id(&self) -> ProviderId {
        ProviderId::Tmdb
    }

    fn search<'a>(
        &'a self,
        kind: MediaKind,
        query: &'a str,
        year: Option<u16>,
        language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<SearchResult>>> {
        Box::pin(async move {
            let (path, year_parameter) = match kind {
                MediaKind::Movie => ("/search/movie", "year"),
                MediaKind::Episode => ("/search/tv", "first_air_date_year"),
            };
            let mut request = self
                .request(path)
                .query("query", query)
                .query("language", language)
                .query("include_adult", "false");
            if let Some(year) = year {
                request = request.query(year_parameter, year.to_string());
            }
            Ok(match kind {
                MediaKind::Movie => convert(self.get::<Page<MovieItem>>(request, true).await?),
                MediaKind::Episode => convert(self.get::<Page<TvItem>>(request, true).await?),
            })
        })
    }

    fn episodes<'a>(
        &'a self,
        series_id: &'a str,
        language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<Episode>>> {
        Box::pin(async move {
            let details: TvDetails = self
                .get(
                    self.request(&format!("/tv/{series_id}"))
                        .query("language", language),
                    true,
                )
                .await?;

            let mut episodes = Vec::new();
            for season in details.seasons {
                let path = format!("/tv/{series_id}/season/{}", season.season_number);
                let season: Season = self
                    .get(self.request(&path).query("language", language), true)
                    .await?;
                episodes.extend(season.episodes.into_iter().map(|e| Episode {
                    season: e.season_number,
                    number: e.episode_number,
                    title: e.name.unwrap_or_default(),
                    air_date: e.air_date.filter(|d| !d.is_empty()),
                }));
            }
            episodes.sort_by_key(|e| (e.season, e.number));
            Ok(episodes)
        })
    }

    fn find_by_imdb_id<'a>(
        &'a self,
        imdb_id: &'a str,
        language: &'a str,
    ) -> BoxFuture<'a, ProviderResult<Vec<SearchResult>>> {
        Box::pin(async move {
            let request = self
                .request(&format!("/find/{imdb_id}"))
                .query("external_source", "imdb_id")
                .query("language", language);
            let found: FindResponse = self.get(request, true).await?;
            Ok(found
                .movie_results
                .into_iter()
                .map(SearchResult::from)
                .chain(found.tv_results.into_iter().map(SearchResult::from))
                .collect())
        })
    }

    fn verify_key(&self) -> BoxFuture<'_, ProviderResult<()>> {
        Box::pin(async move {
            self.fetcher
                .json(&self.request("/authentication"), false)
                .await?;
            Ok(())
        })
    }
}

fn convert<T: Into<SearchResult>>(page: Page<T>) -> Vec<SearchResult> {
    page.results.into_iter().map(Into::into).collect()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::http::HttpError;
    use crate::providers::ProviderError;
    use crate::testing::{fetcher, status, FakeClient};

    const KEY: &str = "chiave-di-prova";

    fn tmdb(client: &Arc<FakeClient>) -> Tmdb {
        Tmdb::new(fetcher(client.clone()), KEY)
    }

    #[tokio::test]
    async fn cerca_un_film() {
        let client = FakeClient::new();
        client.route_json(
            "/search/movie",
            json!({"page": 1, "results": [
                {"id": 27205, "title": "Inception", "original_title": "Inception",
                 "release_date": "2010-07-15"},
                {"id": 64956, "title": "Inception: The Cobol Job",
                 "original_title": "Inception: The Cobol Job", "release_date": ""}
            ]}),
        );
        let found = tmdb(&client)
            .search(MediaKind::Movie, "Inception", Some(2010), "it")
            .await
            .unwrap();

        assert_eq!(
            found[0],
            SearchResult {
                provider: ProviderId::Tmdb,
                id: "27205".to_string(),
                kind: MediaKind::Movie,
                title: "Inception".to_string(),
                original_title: None,
                year: Some(2010),
            }
        );
        assert_eq!(found[1].year, None);

        let request = &client.requests()[0];
        assert_eq!(
            request.describe(),
            "GET https://api.themoviedb.org/3/search/movie\
             ?query=Inception&language=it&include_adult=false&year=2010"
        );
        assert_eq!(
            request.secret_query,
            [("api_key".to_string(), KEY.to_string())]
        );
        assert_eq!(request.bearer, None);
    }

    #[tokio::test]
    async fn cerca_una_serie_con_titolo_originale() {
        let client = FakeClient::new();
        client.route_json(
            "/search/tv",
            json!({"results": [
                {"id": 71446, "name": "La casa di carta", "original_name": "La casa de papel",
                 "first_air_date": "2017-05-02"}
            ]}),
        );
        let found = tmdb(&client)
            .search(MediaKind::Episode, "La casa di carta", Some(2017), "it")
            .await
            .unwrap();
        assert_eq!(found[0].kind, MediaKind::Episode);
        assert_eq!(found[0].original_title.as_deref(), Some("La casa de papel"));
        assert!(client.requests()[0]
            .describe()
            .ends_with("first_air_date_year=2017"));
    }

    #[tokio::test]
    async fn il_token_v4_viaggia_nell_intestazione() {
        let client = FakeClient::new();
        client.route_json("/search/movie", json!({"results": []}));
        let provider = Tmdb::new(fetcher(client.clone()), "aaa.bbb.ccc");
        provider
            .search(MediaKind::Movie, "x", None, "it")
            .await
            .unwrap();
        let request = &client.requests()[0];
        assert_eq!(request.bearer.as_deref(), Some("aaa.bbb.ccc"));
        assert!(request.secret_query.is_empty());
    }

    #[tokio::test]
    async fn elenca_gli_episodi_di_tutte_le_stagioni() {
        let client = FakeClient::new();
        client.route_json(
            "/tv/100/season/1",
            json!({"episodes": [
                {"season_number": 1, "episode_number": 2, "name": "Secondo", "air_date": "2020-01-08"},
                {"season_number": 1, "episode_number": 1, "name": "Primo", "air_date": "2020-01-01"}
            ]}),
        );
        client.route_json(
            "/tv/100/season/2",
            json!({"episodes": [
                {"season_number": 2, "episode_number": 1, "name": null, "air_date": null}
            ]}),
        );
        client.route_json(
            "/tv/100?",
            json!({"id": 100, "name": "Serie", "seasons": [
                {"season_number": 2, "episode_count": 1},
                {"season_number": 1, "episode_count": 2}
            ]}),
        );
        let episodes = tmdb(&client).episodes("100", "it").await.unwrap();
        let summary: Vec<(u32, u32, &str)> = episodes
            .iter()
            .map(|e| (e.season, e.number, e.title.as_str()))
            .collect();
        assert_eq!(summary, [(1, 1, "Primo"), (1, 2, "Secondo"), (2, 1, "")]);
        assert_eq!(episodes[0].air_date.as_deref(), Some("2020-01-01"));
    }

    #[tokio::test]
    async fn trova_per_id_imdb() {
        let client = FakeClient::new();
        client.route_json(
            "/find/tt0111161",
            json!({
                "movie_results": [{"id": 278, "title": "Le ali della libertà",
                    "original_title": "The Shawshank Redemption", "release_date": "1994-09-23"}],
                "tv_results": []
            }),
        );
        let found = tmdb(&client)
            .find_by_imdb_id("tt0111161", "it")
            .await
            .unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].id, "278");
        assert_eq!(found[0].year, Some(1994));
        assert!(client.requests()[0]
            .describe()
            .contains("external_source=imdb_id"));
    }

    #[tokio::test]
    async fn verifica_della_chiave() {
        let client = FakeClient::new();
        client.route_json("/authentication", json!({"success": true}));
        assert_eq!(tmdb(&client).verify_key().await, Ok(()));

        let client = FakeClient::new();
        client.route("/authentication", vec![status(401)]);
        assert_eq!(
            tmdb(&client).verify_key().await,
            Err(ProviderError::Http(HttpError::Unauthorized))
        );
    }

    #[tokio::test]
    async fn risposta_malformata() {
        let client = FakeClient::new();
        client.route_json("/search/movie", json!({"results": "non una lista"}));
        let result = tmdb(&client)
            .search(MediaKind::Movie, "x", None, "it")
            .await;
        assert!(matches!(result, Err(ProviderError::UnexpectedResponse(_))));
    }
}
