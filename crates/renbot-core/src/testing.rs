//! Strumenti condivisi dai test: client HTTP simulato e cartelle temporanee.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;

use crate::http::{
    BoxFuture, Fetcher, HttpClient, HttpError, HttpRequest, HttpResponse, RetryPolicy,
};

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
