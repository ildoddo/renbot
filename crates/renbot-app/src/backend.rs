//! Lavoro in background: ricerche in rete e operazioni sui file girano su un runtime
//! tokio e restituiscono il risultato alla GUI come [`Event`], senza mai bloccarla.

use std::collections::HashMap;
use std::future::Future;
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

use eframe::egui;
use renbot_core::cache::Cache;
use renbot_core::config::keys::{ApiKey, KeyManager};
use renbot_core::config::Settings;
use renbot_core::http::{Fetcher, ReqwestClient, RetryPolicy};
use renbot_core::matcher::{self, ScoredResult};
use renbot_core::parser::{MediaKind, ParsedName};
use renbot_core::providers::tmdb::Tmdb;
use renbot_core::providers::tvdb::Tvdb;
use renbot_core::providers::{self, omdb, Episode, MetadataProvider, ProviderId, SearchResult};
use renbot_core::renamer::{self, ApplyReport, PreviewRow, UndoReport};

use crate::strings;

type Provider = Arc<dyn MetadataProvider>;

/// Vista che ha chiesto una ricerca di serie o una rinomina.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Origin {
    Library,
    Episodes,
    Lists,
}

/// Esito dell'abbinamento automatico di un file.
pub struct MatchData {
    /// Candidati ordinati per punteggio; vuoto se la ricerca non ha trovato nulla.
    pub candidates: Vec<ScoredResult>,
    /// Episodi del primo candidato, se è una serie.
    pub episodes: Vec<Episode>,
}

pub enum Event {
    Matched {
        id: u64,
        generation: u64,
        result: Result<MatchData, String>,
    },
    ManualResults {
        id: u64,
        result: Result<Vec<ScoredResult>, String>,
    },
    Resolved {
        ids: Vec<u64>,
        choice: ScoredResult,
        result: Result<Vec<Episode>, String>,
    },
    SeriesFound {
        origin: Origin,
        result: Result<Vec<SearchResult>, String>,
    },
    EpisodesLoaded {
        origin: Origin,
        series: SearchResult,
        result: Result<Vec<Episode>, String>,
    },
    KeyVerified {
        key: ApiKey,
        result: Result<(), String>,
    },
    Applied {
        origin: Origin,
        result: Result<ApplyReport, String>,
    },
    Undone {
        result: Result<UndoReport, String>,
    },
}

pub struct Backend {
    runtime: tokio::runtime::Runtime,
    fetcher: Arc<Fetcher>,
    cache: Option<Cache>,
    keys: KeyManager,
    /// Provider già costruiti, con la chiave usata: TheTVDB conserva così il token di sessione.
    providers: Mutex<HashMap<ProviderId, (String, Provider)>>,
    events: Sender<Event>,
    ctx: egui::Context,
}

fn api_key(id: ProviderId) -> ApiKey {
    match id {
        ProviderId::Tmdb => ApiKey::Tmdb,
        ProviderId::Tvdb => ApiKey::Tvdb,
    }
}

fn other(id: ProviderId) -> ProviderId {
    match id {
        ProviderId::Tmdb => ProviderId::Tvdb,
        ProviderId::Tvdb => ProviderId::Tmdb,
    }
}

impl Backend {
    pub fn new(ctx: egui::Context) -> anyhow::Result<(Self, Receiver<Event>)> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()?;
        let cache = Cache::in_default_dir();
        let client = Arc::new(ReqwestClient::new()?);
        let fetcher = Arc::new(Fetcher::new(client, cache.clone(), RetryPolicy::default()));
        let keys = KeyManager::system()
            .ok_or_else(|| anyhow::anyhow!("cartella di configurazione non disponibile"))?;
        let (events, receiver) = channel();
        let backend = Self {
            runtime,
            fetcher,
            cache,
            keys,
            providers: Mutex::new(HashMap::new()),
            events,
            ctx,
        };
        Ok((backend, receiver))
    }

    pub fn keys(&self) -> &KeyManager {
        &self.keys
    }

    pub fn has_key(&self, id: ProviderId) -> bool {
        self.keys.get(api_key(id)).is_some()
    }

    pub fn clear_cache(&self) -> std::io::Result<()> {
        match &self.cache {
            Some(cache) => cache.clear(),
            None => Ok(()),
        }
    }

    fn provider(&self, id: ProviderId) -> Result<Provider, String> {
        let (key, _) = self
            .keys
            .get(api_key(id))
            .ok_or_else(|| strings::missing_key(id))?;
        let mut providers = self.providers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((used, provider)) = providers.get(&id) {
            if *used == key {
                return Ok(provider.clone());
            }
        }
        let provider: Provider = match id {
            ProviderId::Tmdb => Arc::new(Tmdb::new(self.fetcher.clone(), key.clone())),
            ProviderId::Tvdb => Arc::new(Tvdb::new(self.fetcher.clone(), key.clone())),
        };
        providers.insert(id, (key, provider.clone()));
        Ok(provider)
    }

    /// Database principale e, se richiesto e configurato, quello di riserva.
    fn pair(&self, settings: &Settings) -> Result<(Provider, Option<Provider>), String> {
        let primary = self.provider(settings.provider)?;
        let fallback = settings
            .use_fallback
            .then(|| self.provider(other(settings.provider)).ok())
            .flatten();
        Ok((primary, fallback))
    }

    fn spawn(&self, task: impl Future<Output = Event> + Send + 'static) {
        let events = self.events.clone();
        let ctx = self.ctx.clone();
        self.runtime.spawn(async move {
            let _ = events.send(task.await);
            ctx.request_repaint();
        });
    }

    fn spawn_blocking(&self, task: impl FnOnce() -> Event + Send + 'static) {
        let events = self.events.clone();
        let ctx = self.ctx.clone();
        self.runtime.spawn_blocking(move || {
            let _ = events.send(task());
            ctx.request_repaint();
        });
    }

    /// Abbinamento automatico: ricerca, punteggio ed episodi del miglior candidato.
    pub fn match_file(&self, settings: &Settings, id: u64, generation: u64, parsed: ParsedName) {
        let pair = self.pair(settings);
        let language = settings.language.clone();
        self.spawn(async move {
            let result = async {
                let (primary, fallback) = pair?;
                let (source, found) = providers::search(
                    primary.as_ref(),
                    fallback.as_deref(),
                    parsed.kind(),
                    &parsed.title,
                    parsed.year,
                    &language,
                )
                .await
                .map_err(|e| e.to_string())?;
                let candidates = matcher::rank(&parsed.title, parsed.year, found);
                let mut episodes = Vec::new();
                if let Some(best) = candidates
                    .first()
                    .filter(|c| c.result.kind == MediaKind::Episode)
                {
                    let provider = match fallback {
                        Some(fallback) if fallback.id() == source => fallback,
                        _ => primary,
                    };
                    episodes = providers::episodes_localized(
                        provider.as_ref(),
                        &best.result.id,
                        &language,
                    )
                    .await
                    .map_err(|e| e.to_string())?;
                }
                Ok(MatchData {
                    candidates,
                    episodes,
                })
            }
            .await;
            Event::Matched {
                id,
                generation,
                result,
            }
        });
    }

    /// Ricerca manuale per titolo o per ID IMDb (`tt…`, sempre tramite TMDB).
    pub fn manual_search(&self, settings: &Settings, id: u64, query: String, parsed: ParsedName) {
        let imdb = is_imdb_id(&query);
        let pair = if imdb {
            self.provider(ProviderId::Tmdb).map(|tmdb| (tmdb, None))
        } else {
            self.pair(settings)
        };
        let language = settings.language.clone();
        self.spawn(async move {
            let result = async {
                let (primary, fallback) = pair?;
                let found = if imdb {
                    primary.find_by_imdb_id(query.trim(), &language).await
                } else {
                    providers::search(
                        primary.as_ref(),
                        fallback.as_deref(),
                        parsed.kind(),
                        &query,
                        None,
                        &language,
                    )
                    .await
                    .map(|(_, found)| found)
                }
                .map_err(|e| e.to_string())?;
                Ok(matcher::rank(&query, parsed.year, found))
            }
            .await;
            Event::ManualResults { id, result }
        });
    }

    /// Episodi del candidato scelto a mano, per le righe indicate.
    pub fn resolve(&self, settings: &Settings, ids: Vec<u64>, choice: ScoredResult) {
        let provider = self.provider(choice.result.provider);
        let language = settings.language.clone();
        self.spawn(async move {
            let result = async {
                if choice.result.kind == MediaKind::Movie {
                    return Ok(Vec::new());
                }
                providers::episodes_localized(provider?.as_ref(), &choice.result.id, &language)
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            Event::Resolved {
                ids,
                choice,
                result,
            }
        });
    }

    pub fn search_series(&self, settings: &Settings, origin: Origin, query: String) {
        let pair = self.pair(settings);
        let language = settings.language.clone();
        self.spawn(async move {
            let result = async {
                let (primary, fallback) = pair?;
                let (_, found) = providers::search(
                    primary.as_ref(),
                    fallback.as_deref(),
                    MediaKind::Episode,
                    &query,
                    None,
                    &language,
                )
                .await
                .map_err(|e| e.to_string())?;
                let ranked = matcher::rank(&query, None, found);
                Ok(ranked.into_iter().map(|scored| scored.result).collect())
            }
            .await;
            Event::SeriesFound { origin, result }
        });
    }

    pub fn load_episodes(&self, settings: &Settings, origin: Origin, series: SearchResult) {
        let provider = self.provider(series.provider);
        let language = settings.language.clone();
        self.spawn(async move {
            let result = async {
                providers::episodes_localized(provider?.as_ref(), &series.id, &language)
                    .await
                    .map_err(|e| e.to_string())
            }
            .await;
            Event::EpisodesLoaded {
                origin,
                series,
                result,
            }
        });
    }

    /// Prova la chiave scritta nel campo con una chiamata reale, anche se non è ancora salvata.
    pub fn verify_key(&self, key: ApiKey, value: String) {
        let fetcher = self.fetcher.clone();
        self.spawn(async move {
            let value = value.trim().to_string();
            let result = match key {
                ApiKey::Tmdb => Tmdb::new(fetcher, value).verify_key().await,
                ApiKey::Tvdb => Tvdb::new(fetcher, value).verify_key().await,
                ApiKey::Omdb => omdb::verify_key(&fetcher, &value).await,
            };
            let result = result.map_err(|e| e.to_string());
            match &result {
                Ok(()) => tracing::info!(key = key.id(), "chiave verificata"),
                Err(error) => {
                    tracing::warn!(key = key.id(), %error, "verifica della chiave non riuscita")
                }
            }
            Event::KeyVerified { key, result }
        });
    }

    pub fn apply(&self, origin: Origin, rows: Vec<PreviewRow>, undo_log: PathBuf) {
        self.spawn_blocking(move || Event::Applied {
            origin,
            result: renamer::apply(&rows, &undo_log).map_err(|e| e.to_string()),
        });
    }

    pub fn undo(&self, undo_log: PathBuf) {
        self.spawn_blocking(move || Event::Undone {
            result: renamer::undo(&undo_log).map_err(|e| e.to_string()),
        });
    }
}

fn is_imdb_id(query: &str) -> bool {
    let query = query.trim();
    query.len() > 4 && query.starts_with("tt") && query[2..].chars().all(|c| c.is_ascii_digit())
}
