//! Accesso HTTP: client astratto (sostituibile nei test), retry con backoff e cache.
//!
//! Le chiavi API viaggiano solo in `secret_query`, `bearer` e `json_body`, che non
//! compaiono mai nei log, nei messaggi di errore né nella chiave di cache.

use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;

use crate::cache::Cache;

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Attesa massima accettata da un'intestazione `Retry-After`.
const MAX_RETRY_AFTER: Duration = Duration::from_secs(30);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    Get,
    Post,
}

#[derive(Clone)]
pub struct HttpRequest {
    pub method: Method,
    pub url: String,
    pub query: Vec<(String, String)>,
    /// Parametri riservati, ad esempio la chiave API.
    pub secret_query: Vec<(String, String)>,
    pub bearer: Option<String>,
    pub json_body: Option<Value>,
}

impl HttpRequest {
    pub fn get(url: impl Into<String>) -> Self {
        Self {
            method: Method::Get,
            url: url.into(),
            query: Vec::new(),
            secret_query: Vec::new(),
            bearer: None,
            json_body: None,
        }
    }

    pub fn post(url: impl Into<String>, body: Value) -> Self {
        Self {
            method: Method::Post,
            json_body: Some(body),
            ..Self::get(url)
        }
    }

    pub fn query(mut self, name: &str, value: impl Into<String>) -> Self {
        self.query.push((name.to_string(), value.into()));
        self
    }

    pub fn secret_query(mut self, name: &str, value: impl Into<String>) -> Self {
        self.secret_query.push((name.to_string(), value.into()));
        self
    }

    pub fn bearer(mut self, token: impl Into<String>) -> Self {
        self.bearer = Some(token.into());
        self
    }

    /// Descrizione della richiesta priva di segreti, adatta ai log.
    pub fn describe(&self) -> String {
        let method = match self.method {
            Method::Get => "GET",
            Method::Post => "POST",
        };
        if self.query.is_empty() {
            return format!("{method} {}", self.url);
        }
        let query: Vec<String> = self.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
        format!("{method} {}?{}", self.url, query.join("&"))
    }

    /// Chiave di cache: indirizzo e parametri pubblici, in ordine stabile.
    pub fn cache_key(&self) -> String {
        let mut query: Vec<String> = self.query.iter().map(|(k, v)| format!("{k}={v}")).collect();
        query.sort();
        format!("{}?{}", self.url, query.join("&"))
    }
}

impl fmt::Debug for HttpRequest {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.describe())
    }
}

#[derive(Debug, Clone)]
pub struct HttpResponse {
    pub status: u16,
    pub retry_after: Option<Duration>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum HttpError {
    #[error("errore di rete: {0}")]
    Transport(String),
    #[error("chiave API mancante, non valida o non autorizzata")]
    Unauthorized,
    #[error("risorsa non trovata")]
    NotFound,
    #[error("il server ha risposto con lo stato {0}")]
    Status(u16),
    #[error("risposta non valida: {0}")]
    InvalidBody(String),
}

pub trait HttpClient: Send + Sync {
    fn send<'a>(
        &'a self,
        request: &'a HttpRequest,
    ) -> BoxFuture<'a, Result<HttpResponse, HttpError>>;
}

/// Client reale basato su `reqwest` con TLS `rustls`.
pub struct ReqwestClient {
    inner: reqwest::Client,
}

impl ReqwestClient {
    pub fn new() -> Result<Self, HttpError> {
        let inner = reqwest::Client::builder()
            .user_agent(concat!("renbot/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(20))
            .build()
            .map_err(transport)?;
        Ok(Self { inner })
    }
}

/// Converte un errore di `reqwest` togliendo l'indirizzo, che può contenere la chiave API.
fn transport(error: reqwest::Error) -> HttpError {
    HttpError::Transport(error.without_url().to_string())
}

impl HttpClient for ReqwestClient {
    fn send<'a>(
        &'a self,
        request: &'a HttpRequest,
    ) -> BoxFuture<'a, Result<HttpResponse, HttpError>> {
        Box::pin(async move {
            let mut builder = match request.method {
                Method::Get => self.inner.get(&request.url),
                Method::Post => self.inner.post(&request.url),
            }
            .query(&request.query)
            .query(&request.secret_query);
            if let Some(token) = &request.bearer {
                builder = builder.bearer_auth(token);
            }
            if let Some(body) = &request.json_body {
                builder = builder
                    .header(reqwest::header::CONTENT_TYPE, "application/json")
                    .body(body.to_string());
            }

            let response = builder.send().await.map_err(transport)?;
            let status = response.status().as_u16();
            let retry_after = response
                .headers()
                .get(reqwest::header::RETRY_AFTER)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| value.trim().parse().ok())
                .map(Duration::from_secs);
            let body = response.text().await.map_err(transport)?;
            Ok(HttpResponse {
                status,
                retry_after,
                body,
            })
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RetryPolicy {
    /// Tentativi aggiuntivi dopo il primo.
    pub max_retries: u32,
    /// Attesa prima del primo nuovo tentativo; raddoppia a ogni tentativo.
    pub base_delay: Duration,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(500),
        }
    }
}

/// Esegue le richieste con retry e, per le GET, con cache su disco.
pub struct Fetcher {
    client: Arc<dyn HttpClient>,
    cache: Option<Cache>,
    retry: RetryPolicy,
}

impl Fetcher {
    pub fn new(client: Arc<dyn HttpClient>, cache: Option<Cache>, retry: RetryPolicy) -> Self {
        Self {
            client,
            cache,
            retry,
        }
    }

    /// Restituisce il corpo JSON della risposta. Con `use_cache` le GET sono lette
    /// dalla cache, se presenti e non scadute, e vi sono salvate dopo il download.
    pub async fn json(&self, request: &HttpRequest, use_cache: bool) -> Result<Value, HttpError> {
        let cache = self
            .cache
            .as_ref()
            .filter(|_| use_cache && request.method == Method::Get);
        let key = request.cache_key();

        if let Some(body) = cache.and_then(|c| c.get(&key)) {
            if let Ok(value) = serde_json::from_str(&body) {
                return Ok(value);
            }
        }

        let body = self.send_with_retry(request).await?;
        let value =
            serde_json::from_str(&body).map_err(|e| HttpError::InvalidBody(e.to_string()))?;
        if let Some(cache) = cache {
            if let Err(error) = cache.put(&key, &body) {
                tracing::warn!(%error, "impossibile scrivere nella cache");
            }
        }
        Ok(value)
    }

    async fn send_with_retry(&self, request: &HttpRequest) -> Result<String, HttpError> {
        let mut attempt = 0;
        loop {
            let (error, server_delay) = match self.client.send(request).await {
                Ok(response) => match response.status {
                    200..=299 => return Ok(response.body),
                    401 | 403 => return Err(HttpError::Unauthorized),
                    404 => return Err(HttpError::NotFound),
                    429 | 500..=599 => (HttpError::Status(response.status), response.retry_after),
                    other => return Err(HttpError::Status(other)),
                },
                Err(error @ HttpError::Transport(_)) => (error, None),
                Err(error) => return Err(error),
            };

            if attempt >= self.retry.max_retries {
                return Err(error);
            }
            let backoff = self.retry.base_delay * 2u32.pow(attempt);
            let delay = server_delay.map_or(backoff, |d| d.min(MAX_RETRY_AFTER));
            attempt += 1;
            tracing::warn!(
                request = %request.describe(),
                %error,
                attempt,
                "richiesta fallita, nuovo tentativo"
            );
            tokio::time::sleep(delay).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::cache::DEFAULT_TTL;
    use crate::testing::{fetcher, ok, status, FakeClient, TempDir};

    #[tokio::test]
    async fn riprova_dopo_429_e_poi_riesce() {
        let client = FakeClient::new();
        let limited = HttpResponse {
            retry_after: Some(Duration::ZERO),
            ..status(429)
        };
        client.route(
            "/dati",
            vec![limited, status(503), ok(json!({"valore": 7}))],
        );
        let value = fetcher(client.clone())
            .json(&HttpRequest::get("https://esempio.test/dati"), true)
            .await
            .unwrap();
        assert_eq!(value["valore"], 7);
        assert_eq!(client.count("/dati"), 3);
    }

    #[tokio::test]
    async fn rinuncia_dopo_il_numero_massimo_di_tentativi() {
        let client = FakeClient::new();
        client.route("/dati", vec![status(500)]);
        let result = fetcher(client.clone())
            .json(&HttpRequest::get("https://esempio.test/dati"), true)
            .await;
        assert_eq!(result, Err(HttpError::Status(500)));
        assert_eq!(client.count("/dati"), 4);
    }

    #[tokio::test]
    async fn non_riprova_con_chiave_non_valida() {
        let client = FakeClient::new();
        client.route("/dati", vec![status(401)]);
        let result = fetcher(client.clone())
            .json(&HttpRequest::get("https://esempio.test/dati"), true)
            .await;
        assert_eq!(result, Err(HttpError::Unauthorized));
        assert_eq!(client.count("/dati"), 1);
    }

    #[tokio::test]
    async fn la_seconda_richiesta_arriva_dalla_cache() {
        let dir = TempDir::new("http-cache");
        let client = FakeClient::new();
        client.route_json("/dati", json!({"valore": 1}));
        let fetcher = Fetcher::new(
            client.clone(),
            Some(Cache::new(dir.path(), DEFAULT_TTL)),
            RetryPolicy::default(),
        );
        let request = HttpRequest::get("https://esempio.test/dati").secret_query("api_key", "K");

        fetcher.json(&request, true).await.unwrap();
        fetcher.json(&request, true).await.unwrap();
        assert_eq!(client.count("/dati"), 1);

        fetcher.json(&request, false).await.unwrap();
        assert_eq!(client.count("/dati"), 2);
    }

    #[test]
    fn i_segreti_non_compaiono_in_log_e_cache() {
        let request = HttpRequest::post("https://esempio.test/login", json!({"apikey": "SEGRETO"}))
            .query("lingua", "it")
            .secret_query("api_key", "SEGRETO")
            .bearer("SEGRETO");
        for text in [
            request.describe(),
            request.cache_key(),
            format!("{request:?}"),
        ] {
            assert!(!text.contains("SEGRETO"), "{text}");
            assert!(text.contains("lingua=it"));
        }
    }
}
