//! Strumenti condivisi dai test: client HTTP e database simulati, cartelle temporanee.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::http::{
    BoxFuture, Fetcher, HttpClient, HttpError, HttpRequest, HttpResponse, RetryPolicy,
};
use crate::parser::MediaKind;
use crate::providers::{Episode, MetadataProvider, ProviderError, ProviderId, SearchResult};

/// Client che risponde con dati predefiniti e registra le richieste ricevute.
#[derive(Default)]
pub struct FakeClient {
    routes: Mutex<Vec<(String, VecDeque<HttpResponse>)>>,
    requests: Mutex<Vec<HttpRequest>>,
}

impl FakeClient {
    pub fn new() -> Arc<Self> {
        Arc::new(Self::default())
    }

    /// Registra le risposte per le richieste la cui descrizione contiene `pattern`.
    /// Le rotte sono valutate in ordine; l'ultima risposta di una rotta si ripete.
    pub fn route(&self, pattern: &str, responses: Vec<HttpResponse>) {
        self.routes
            .lock()
            .unwrap()
            .push((pattern.to_string(), responses.into()));
    }

    pub fn route_json(&self, pattern: &str, body: Value) {
        self.route(pattern, vec![ok(body)]);
    }

    pub fn requests(&self) -> Vec<HttpRequest> {
        self.requests.lock().unwrap().clone()
    }

    pub fn count(&self, pattern: &str) -> usize {
        self.requests()
            .iter()
            .filter(|r| r.describe().contains(pattern))
            .count()
    }
}

impl HttpClient for FakeClient {
    fn send<'a>(
        &'a self,
        request: &'a HttpRequest,
    ) -> BoxFuture<'a, Result<HttpResponse, HttpError>> {
        self.requests.lock().unwrap().push(request.clone());
        let description = request.describe();
        let mut routes = self.routes.lock().unwrap();
        let response = routes
            .iter_mut()
            .find(|(pattern, _)| description.contains(pattern.as_str()))
            .and_then(|(_, queue)| {
                if queue.len() > 1 {
                    queue.pop_front()
                } else {
                    queue.front().cloned()
                }
            })
            .unwrap_or_else(|| status(404));
        Box::pin(async move { Ok(response) })
    }
}

pub fn ok(body: Value) -> HttpResponse {
    HttpResponse {
        status: 200,
        retry_after: None,
        body: body.to_string(),
    }
}

pub fn status(code: u16) -> HttpResponse {
    HttpResponse {
        status: code,
        retry_after: None,
        body: String::new(),
    }
}

/// Fetcher senza cache e senza attese tra i tentativi.
pub fn fetcher(client: Arc<FakeClient>) -> Arc<Fetcher> {
    Arc::new(Fetcher::new(
        client,
        None,
        RetryPolicy {
            max_retries: 3,
            base_delay: Duration::ZERO,
        },
    ))
}

/// Cartella temporanea eliminata alla fine del test.
pub struct TempDir(PathBuf);

impl TempDir {
    pub fn new(name: &str) -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let unique = format!(
            "renbot-test-{name}-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(unique);
        std::fs::create_dir_all(&path).unwrap();
        Self(path)
    }

    pub fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Database simulato: restituisce risultati ed episodi fissi, senza rete.
pub struct FakeProvider {
    pub id: ProviderId,
    /// Risultati di ricerca; a ogni richiesta sono restituiti quelli del tipo cercato.
    pub results: Vec<SearchResult>,
    /// Episodi restituiti per qualsiasi serie.
    pub episodes: Vec<Episode>,
}

impl FakeProvider {
    pub fn new(id: ProviderId) -> Self {
        Self {
            id,
            results: Vec::new(),
            episodes: Vec::new(),
        }
    }

    pub fn with_result(mut self, kind: MediaKind, id: &str, title: &str, year: u16) -> Self {
        self.results.push(SearchResult {
            provider: self.id,
            id: id.to_string(),
            kind,
            title: title.to_string(),
            original_title: None,
            year: Some(year),
        });
        self
    }

    pub fn with_episode(mut self, season: u32, number: u32, title: &str) -> Self {
        self.episodes.push(Episode {
            season,
            number,
            title: title.to_string(),
            air_date: Some(format!("2020-{season:02}-{number:02}")),
        });
        self
    }
}

impl MetadataProvider for FakeProvider {
    fn id(&self) -> ProviderId {
        self.id
    }

    fn search<'a>(
        &'a self,
        kind: MediaKind,
        _query: &'a str,
        _year: Option<u16>,
        _language: &'a str,
    ) -> BoxFuture<'a, Result<Vec<SearchResult>, ProviderError>> {
        let found: Vec<SearchResult> = self
            .results
            .iter()
            .filter(|result| result.kind == kind)
            .cloned()
            .collect();
        Box::pin(async move { Ok(found) })
    }

    fn episodes<'a>(
        &'a self,
        _series_id: &'a str,
        _language: &'a str,
    ) -> BoxFuture<'a, Result<Vec<Episode>, ProviderError>> {
        Box::pin(async move { Ok(self.episodes.clone()) })
    }

    fn find_by_imdb_id<'a>(
        &'a self,
        _imdb_id: &'a str,
        _language: &'a str,
    ) -> BoxFuture<'a, Result<Vec<SearchResult>, ProviderError>> {
        Box::pin(async { Err(ProviderError::Unsupported) })
    }

    fn verify_key(&self) -> BoxFuture<'_, Result<(), ProviderError>> {
        Box::pin(async { Ok(()) })
    }
}
