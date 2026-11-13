//! Riga di comando di renbot: gli stessi passi dell'interfaccia grafica, da terminale.
//!
//! Per impostazione predefinita i comandi mostrano solo un'anteprima; i file sono
//! rinominati soltanto con `rename --apply` o `list apply`.

mod text;

use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Context};
use clap::{Args, Parser, Subcommand, ValueEnum};
use renbot_core::cache::Cache;
use renbot_core::config::keys::{ApiKey, KeyManager};
use renbot_core::config::Settings;
use renbot_core::http::{BoxFuture, Fetcher, ReqwestClient, RetryPolicy};
use renbot_core::matcher;
use renbot_core::parser::{self, MediaKind};
use renbot_core::providers::tmdb::Tmdb;
use renbot_core::providers::tvdb::Tvdb;
use renbot_core::providers::{self, omdb, MetadataProvider, ProviderId};
use renbot_core::renamer::format::unknown_tokens;
use renbot_core::renamer::list::RenameList;
use renbot_core::renamer::{self, PreviewRow, PreviewStatus, RenameItem};
use renbot_core::scanner;
use renbot_core::workflow;

/// Rinomina film, serie TV e anime con i dati di TMDB e TVDB.
#[derive(Debug, Parser)]
#[command(name = "renbot", version, arg_required_else_help = true)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Abbina i file ai titoli e mostra l'anteprima; rinomina solo con --apply
    Rename {
        /// File video o cartelle (esplorate in profondità)
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Rinomina i file pronti invece di mostrare solo l'anteprima
        #[arg(long)]
        apply: bool,
        /// Rinomina anche gli abbinamenti "Da verificare"
        #[arg(long)]
        include_review: bool,
        #[command(flatten)]
        search: SearchOptions,
        /// Formato dei nomi dei film, al posto di quello salvato
        #[arg(long, value_name = "FORMATO")]
        movie_format: Option<String>,
        /// Formato dei nomi degli episodi, al posto di quello salvato
        #[arg(long, value_name = "FORMATO")]
        series_format: Option<String>,
    },
    /// Annulla l'ultima rinomina, fatta da terminale o dalla finestra
    Undo,
    /// Cerca una serie e ne stampa l'elenco degli episodi
    Episodes {
        /// Titolo della serie
        query: String,
        /// Mostra solo questa stagione
        #[arg(long, value_name = "N")]
        season: Option<u32>,
        /// Quale risultato usare, se il primo non è quello giusto
        #[arg(long, value_name = "N", default_value_t = 1)]
        choice: usize,
        #[command(flatten)]
        search: SearchOptions,
    },
    /// Liste di rinomine salvate dalla sezione Liste
    List {
        #[command(subcommand)]
        action: ListAction,
    },
    /// Chiavi API dei database
    Keys {
        #[command(subcommand)]
        action: KeyAction,
    },
    /// Mostra che cosa viene riconosciuto dal nome di un file
    Parse {
        /// Nome del file, senza cartelle
        name: String,
    },
}

#[derive(Debug, Args)]
pub struct SearchOptions {
    /// Database principale, al posto di quello salvato
    #[arg(long, value_enum)]
    pub db: Option<Db>,
    /// Non usare l'altro database quando il principale non trova nulla
    #[arg(long)]
    pub no_fallback: bool,
    /// Lingua di titoli ed episodi, ad esempio it o en
    #[arg(long, value_name = "CODICE")]
    pub lang: Option<String>,
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum Db {
    Tmdb,
    Tvdb,
}

#[derive(Debug, Subcommand)]
pub enum ListAction {
    /// Mostra l'anteprima di una lista senza modificare nulla
    Show(ListFile),
    /// Rinomina le righe pronte di una lista
    Apply(ListFile),
}

#[derive(Debug, Args)]
pub struct ListFile {
    /// File JSON della lista
    pub file: PathBuf,
    /// Formato dei nomi degli episodi, al posto di quello salvato
    #[arg(long, value_name = "FORMATO")]
    pub series_format: Option<String>,
}

#[derive(Debug, Subcommand)]
pub enum KeyAction {
    /// Salva una chiave, letta dalla tastiera o dallo standard input
    Set { key: KeyName },
    /// Verifica la chiave salvata con una chiamata reale
    Verify { key: KeyName },
    /// Cancella la chiave salvata
    Delete { key: KeyName },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
pub enum KeyName {
    Tmdb,
    Tvdb,
    Omdb,
}

impl From<KeyName> for ApiKey {
    fn from(name: KeyName) -> Self {
        match name {
            KeyName::Tmdb => ApiKey::Tmdb,
            KeyName::Tvdb => ApiKey::Tvdb,
            KeyName::Omdb => ApiKey::Omdb,
        }
    }
}

type Provider = Arc<dyn MetadataProvider>;

/// Accesso ai database: reale in produzione, simulato nei test.
pub trait Backend {
    fn provider(&self, id: ProviderId) -> Result<Provider, String>;
    fn verify_key<'a>(&'a self, key: ApiKey, value: &'a str) -> BoxFuture<'a, Result<(), String>>;
}

/// Ciò che serve ai comandi per lavorare.
pub struct App<'a> {
    pub settings: Settings,
    pub keys: &'a KeyManager,
    pub backend: &'a dyn Backend,
    pub undo_log: PathBuf,
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

/// Database reali, con le chiavi lette dall'archivio dell'utente.
struct SystemBackend<'a> {
    fetcher: Arc<Fetcher>,
    keys: &'a KeyManager,
    /// Un'istanza per database: TheTVDB conserva così il token di sessione tra un file e l'altro.
    providers: Mutex<HashMap<ProviderId, Provider>>,
}

impl Backend for SystemBackend<'_> {
    fn provider(&self, id: ProviderId) -> Result<Provider, String> {
        let mut providers = self.providers.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(provider) = providers.get(&id) {
            return Ok(provider.clone());
        }
        let (key, _) = self
            .keys
            .get(api_key(id))
            .ok_or_else(|| text::missing_key(id))?;
        let provider: Provider = match id {
            ProviderId::Tmdb => Arc::new(Tmdb::new(self.fetcher.clone(), key)),
            ProviderId::Tvdb => Arc::new(Tvdb::new(self.fetcher.clone(), key)),
        };
        providers.insert(id, provider.clone());
        Ok(provider)
    }

    fn verify_key<'a>(&'a self, key: ApiKey, value: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            let fetcher = self.fetcher.clone();
            match key {
                ApiKey::Tmdb => Tmdb::new(fetcher, value).verify_key().await,
                ApiKey::Tvdb => Tvdb::new(fetcher, value).verify_key().await,
                ApiKey::Omdb => omdb::verify_key(&fetcher, value).await,
            }
            .map_err(|e| e.to_string())
        })
    }
}

/// Punto d'ingresso degli eseguibili: legge gli argomenti del processo ed esegue il comando.
pub fn main_from_env() -> ExitCode {
    let cli = Cli::parse_from(std::env::args_os());
    match run_system(cli) {
        Ok(true) => ExitCode::SUCCESS,
        Ok(false) => ExitCode::FAILURE,
        Err(error) => {
            eprintln!("{}: {error:#}", text::ERROR);
            ExitCode::FAILURE
        }
    }
}

/// Vero se gli argomenti chiedono la riga di comando invece della finestra.
pub fn wants_cli(args: &[OsString]) -> bool {
    args.len() > 1
}

fn run_system(cli: Cli) -> anyhow::Result<bool> {
    let settings = Settings::default_path()
        .map(|path| Settings::load(&path))
        .unwrap_or_default();
    let keys = KeyManager::system().context("cartella di configurazione non disponibile")?;
    let undo_log = renamer::default_undo_log().context("cartella dei dati non disponibile")?;
    let client = Arc::new(ReqwestClient::new()?);
    let fetcher = Arc::new(Fetcher::new(
        client,
        Cache::in_default_dir(),
        RetryPolicy::default(),
    ));
    let backend = SystemBackend {
        fetcher,
        keys: &keys,
        providers: Mutex::new(HashMap::new()),
    };
    let app = App {
        settings,
        keys: &keys,
        backend: &backend,
        undo_log,
    };
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;
    let stdin = std::io::stdin();
    runtime.block_on(run(
        cli.command,
        &app,
        &mut std::io::stdout(),
        &mut stdin.lock(),
    ))
}

/// Esegue un comando. Restituisce `false` se qualcosa non è andato a buon fine
/// (un file non rinominato, una chiave non valida), `Err` per gli errori che lo interrompono.
pub async fn run(
    command: Command,
    app: &App<'_>,
    out: &mut dyn Write,
    input: &mut dyn BufRead,
) -> anyhow::Result<bool> {
    match command {
        Command::Rename {
            paths,
            apply,
            include_review,
            search,
            movie_format,
            series_format,
        } => {
            let mut settings = search.apply_to(&app.settings);
            if let Some(format) = movie_format {
                settings.movie_format = format;
            }
            if let Some(format) = series_format {
                settings.series_format = format;
            }
            rename(app, &settings, &paths, apply, include_review, out).await
        }
        Command::Undo => undo(app, out),
        Command::Episodes {
            query,
            season,
            choice,
            search,
        } => {
            let settings = search.apply_to(&app.settings);
            episodes(app, &settings, &query, season, choice, out).await
        }
        Command::List { action } => {
            let (file, apply) = match action {
                ListAction::Show(file) => (file, false),
                ListAction::Apply(file) => (file, true),
            };
            let format = file.series_format.as_deref();
            let format = format.unwrap_or(&app.settings.series_format);
            list(app, &file.file, format, apply, out)
        }
        Command::Keys { action } => keys(app, action, out, input).await,
        Command::Parse { name } => {
            print_parsed(&name, out)?;
            Ok(true)
        }
    }
}

impl SearchOptions {
    /// Impostazioni salvate, con le opzioni date sulla riga di comando al posto delle loro.
    fn apply_to(&self, saved: &Settings) -> Settings {
        let mut settings = saved.clone();
        if let Some(db) = self.db {
            settings.provider = match db {
                Db::Tmdb => ProviderId::Tmdb,
                Db::Tvdb => ProviderId::Tvdb,
            };
        }
        if self.no_fallback {
            settings.use_fallback = false;
        }
        if let Some(language) = &self.lang {
            settings.language = language.clone();
        }
        settings
    }
}

/// Database principale e, se richiesto e configurato, quello di riserva.
fn pair(app: &App, settings: &Settings) -> anyhow::Result<(Provider, Option<Provider>)> {
    let primary = app
        .backend
        .provider(settings.provider)
        .map_err(|e| anyhow!(e))?;
    let fallback = settings
        .use_fallback
        .then(|| app.backend.provider(other(settings.provider)).ok())
        .flatten();
    Ok((primary, fallback))
}

/// Esito dell'abbinamento di un file, prima del controllo sul disco.
enum Outcome {
    Target { relative: PathBuf, review: bool },
    NoMatch,
    NoEpisode,
    Failed(String),
}

fn warn_unknown_tokens(formats: &[&str], out: &mut dyn Write) -> anyhow::Result<()> {
    let mut unknown: Vec<String> = formats.iter().flat_map(|f| unknown_tokens(f)).collect();
    unknown.sort();
    unknown.dedup();
    if !unknown.is_empty() {
        let list: Vec<String> = unknown.iter().map(|t| format!("{{{t}}}")).collect();
        writeln!(out, "{} {}", text::UNKNOWN_TOKENS, list.join(" "))?;
    }
    Ok(())
}

async fn rename(
    app: &App<'_>,
    settings: &Settings,
    paths: &[PathBuf],
    apply: bool,
    include_review: bool,
    out: &mut dyn Write,
) -> anyhow::Result<bool> {
    let files = scanner::scan(paths);
    if files.is_empty() {
        writeln!(out, "{}", text::NO_VIDEO)?;
        return Ok(false);
    }
    warn_unknown_tokens(&[&settings.movie_format, &settings.series_format], out)?;
    let (primary, fallback) = pair(app, settings)?;

    let mut outcomes = Vec::new();
    for file in &files {
        let parsed = parser::parse(&file_name(file));
        let found = workflow::find_match(
            primary.as_ref(),
            fallback.as_deref(),
            &parsed,
            &settings.language,
        )
        .await;
        outcomes.push(match found {
            Err(error) => Outcome::Failed(error.to_string()),
            Ok(found) => match found.candidates.first() {
                None => Outcome::NoMatch,
                Some(best) => workflow::relative_target(
                    &parsed,
                    &best.result,
                    &found.episodes,
                    &settings.movie_format,
                    &settings.series_format,
                )
                .map_or(Outcome::NoEpisode, |relative| Outcome::Target {
                    relative,
                    review: best.needs_review(),
                }),
            },
        });
    }

    // Anteprima sul disco per i soli file abbinati.
    let items: Vec<RenameItem> = files
        .iter()
        .zip(&outcomes)
        .filter_map(|(file, outcome)| match outcome {
            Outcome::Target { relative, .. } => Some(RenameItem::new(file, None, relative)),
            _ => None,
        })
        .collect();
    let mut previews = renamer::preview(items).into_iter();

    let mut selected = Vec::new();
    let (mut ready, mut review_count, mut failed) = (0, 0, false);
    for (file, outcome) in files.iter().zip(&outcomes) {
        let name = file_name(file);
        match outcome {
            Outcome::NoMatch => print_row(out, text::NO_MATCH, &name, None, None)?,
            Outcome::NoEpisode => print_row(out, text::NO_EPISODE, &name, None, None)?,
            Outcome::Failed(error) => {
                failed = true;
                print_row(out, text::ERROR, &name, None, Some(error))?;
            }
            Outcome::Target { review, .. } => {
                let Some(row) = previews.next() else { continue };
                let target = relative_to_source(&row.item);
                let (label, note) = match row.status {
                    PreviewStatus::Ready if *review => (text::REVIEW, None),
                    PreviewStatus::Ready => (text::READY, None),
                    status => status_label(status),
                };
                print_row(out, label, &name, Some(&target), note)?;
                if row.status == PreviewStatus::Ready {
                    if *review {
                        review_count += 1;
                    } else {
                        ready += 1;
                    }
                    if !*review || include_review {
                        selected.push(row);
                    }
                }
            }
        }
    }

    let other = files.len() - ready - review_count;
    writeln!(out)?;
    writeln!(
        out,
        "{}",
        text::summary(files.len(), ready, review_count, other)
    )?;
    if review_count > 0 && !include_review {
        writeln!(out, "{}", text::REVIEW_HINT)?;
    }
    if !apply {
        writeln!(out, "{}", text::PREVIEW_ONLY)?;
        return Ok(!failed);
    }
    Ok(apply_rows(app, &selected, out)? && !failed)
}

/// Rinomina le righe scelte e ne stampa l'esito.
fn apply_rows(app: &App, rows: &[PreviewRow], out: &mut dyn Write) -> anyhow::Result<bool> {
    if rows.is_empty() {
        writeln!(out, "{}", text::NOTHING_TO_RENAME)?;
        return Ok(true);
    }
    let report = renamer::apply(rows, &app.undo_log)?;
    for (item, error) in &report.failed {
        let name = file_name(&item.source);
        print_row(out, text::ERROR, &name, None, Some(&error.to_string()))?;
    }
    writeln!(
        out,
        "{}",
        text::renamed(report.renamed.len(), report.failed.len())
    )?;
    Ok(report.failed.is_empty())
}

fn undo(app: &App, out: &mut dyn Write) -> anyhow::Result<bool> {
    if !renamer::can_undo(&app.undo_log) {
        writeln!(out, "{}", text::NOTHING_TO_UNDO)?;
        return Ok(true);
    }
    let report = renamer::undo(&app.undo_log)?;
    for item in &report.restored {
        let restored = file_name(&item.source);
        print_row(
            out,
            text::RESTORED,
            &file_name(&item.target),
            Some(&restored),
            None,
        )?;
    }
    for (item, error) in &report.failed {
        let name = file_name(&item.target);
        print_row(out, text::ERROR, &name, None, Some(&error.to_string()))?;
    }
    writeln!(
        out,
        "{}",
        text::undone(report.restored.len(), report.failed.len())
    )?;
    Ok(report.failed.is_empty())
}

async fn episodes(
    app: &App<'_>,
    settings: &Settings,
    query: &str,
    season: Option<u32>,
    choice: usize,
    out: &mut dyn Write,
) -> anyhow::Result<bool> {
    let (primary, fallback) = pair(app, settings)?;
    let (_, found) = providers::search(
        primary.as_ref(),
        fallback.as_deref(),
        MediaKind::Episode,
        query,
        None,
        &settings.language,
    )
    .await?;
    let ranked = matcher::rank(query, None, found);
    let Some(chosen) = choice.checked_sub(1).and_then(|i| ranked.get(i)) else {
        writeln!(out, "{}", text::NO_SERIES)?;
        return Ok(false);
    };
    let series = &chosen.result;
    let provider = app
        .backend
        .provider(series.provider)
        .map_err(|e| anyhow!(e))?;
    let episodes =
        providers::episodes_localized(provider.as_ref(), &series.id, &settings.language).await?;

    writeln!(
        out,
        "{} · {}",
        title_with_year(series),
        text::provider(series.provider)
    )?;
    for episode in episodes
        .iter()
        .filter(|e| season.is_none_or(|s| s == e.season))
    {
        writeln!(
            out,
            "S{:02}E{:02}  {}  {}",
            episode.season,
            episode.number,
            episode.air_date.as_deref().unwrap_or("          "),
            episode.title
        )?;
    }
    if ranked.len() > 1 {
        writeln!(out)?;
        writeln!(out, "{}", text::OTHER_RESULTS)?;
        for (index, scored) in ranked.iter().enumerate() {
            let marker = if index + 1 == choice { "*" } else { " " };
            let result = &scored.result;
            writeln!(
                out,
                "{marker} {}. {} · {}",
                index + 1,
                title_with_year(result),
                text::provider(result.provider)
            )?;
        }
    }
    Ok(true)
}

fn list(
    app: &App,
    file: &Path,
    format: &str,
    apply: bool,
    out: &mut dyn Write,
) -> anyhow::Result<bool> {
    let list = RenameList::load(file)
        .with_context(|| format!("impossibile leggere la lista {}", file.display()))?;
    warn_unknown_tokens(&[format], out)?;
    writeln!(
        out,
        "{} · {}",
        title_with_year(&list.series),
        text::provider(list.series.provider)
    )?;

    let duplicates = list.duplicate_rows();
    let indexed: Vec<(usize, RenameItem)> = (0..list.rows.len())
        .filter_map(|index| Some((index, list.row_item(index, format, None)?)))
        .collect();
    let previews = renamer::preview(indexed.iter().map(|(_, item)| item.clone()).collect());
    let mut by_row: HashMap<usize, PreviewRow> = indexed
        .iter()
        .map(|(index, _)| *index)
        .zip(previews)
        .collect();

    let mut selected = Vec::new();
    for (index, row) in list.rows.iter().enumerate() {
        let name = file_name(&row.file);
        if !row.included {
            print_row(out, text::EXCLUDED, &name, None, None)?;
        } else if row.episode.is_none() {
            print_row(out, text::UNASSIGNED, &name, None, None)?;
        } else if let Some(preview) = by_row.remove(&index) {
            let target = relative_to_source(&preview.item);
            let (label, note) = if duplicates.contains(&index) {
                (text::CONFLICT, Some(text::DUPLICATE_EPISODE))
            } else if preview.status == PreviewStatus::Ready {
                (text::READY, None)
            } else if preview.status == PreviewStatus::Unchanged {
                (text::RENAMED, None)
            } else {
                status_label(preview.status)
            };
            print_row(out, label, &name, Some(&target), note)?;
            if label == text::READY {
                selected.push(preview);
            }
        } else {
            print_row(out, text::ERROR, &name, None, Some(text::INVALID_TARGET))?;
        }
    }

    writeln!(out)?;
    writeln!(
        out,
        "{}",
        text::list_summary(list.rows.len(), selected.len())
    )?;
    if !apply {
        return Ok(true);
    }
    apply_rows(app, &selected, out)
}

async fn keys(
    app: &App<'_>,
    action: KeyAction,
    out: &mut dyn Write,
    input: &mut dyn BufRead,
) -> anyhow::Result<bool> {
    match action {
        KeyAction::Set { key } => {
            let key = ApiKey::from(key);
            write!(out, "{}", text::KEY_PROMPT)?;
            out.flush()?;
            let mut value = String::new();
            input.read_line(&mut value)?;
            if value.trim().is_empty() {
                writeln!(out, "{}", text::KEY_EMPTY)?;
                return Ok(false);
            }
            let source = app.keys.set(key, &value)?;
            writeln!(out, "{}", text::key_saved(source, key))?;
            Ok(true)
        }
        KeyAction::Verify { key } => {
            let key = ApiKey::from(key);
            let Some((value, _)) = app.keys.get(key) else {
                writeln!(out, "{}", text::KEY_MISSING)?;
                return Ok(false);
            };
            match app.backend.verify_key(key, &value).await {
                Ok(()) => {
                    writeln!(out, "{}", text::KEY_VALID)?;
                    Ok(true)
                }
                Err(error) => {
                    writeln!(out, "{}: {error}", text::ERROR)?;
                    Ok(false)
                }
            }
        }
        KeyAction::Delete { key } => {
            app.keys.delete(key.into())?;
            writeln!(out, "{}", text::KEY_DELETED)?;
            Ok(true)
        }
    }
}

fn print_parsed(name: &str, out: &mut dyn Write) -> std::io::Result<()> {
    let parsed = parser::parse(name);
    let kind = match parsed.kind() {
        MediaKind::Movie => "film",
        MediaKind::Episode => "episodio",
    };
    let number = |value: Option<u32>| value.map(|v| v.to_string());
    let fields = [
        ("tipo", Some(kind.to_string())),
        ("titolo", Some(parsed.title.clone())),
        ("anno", parsed.year.map(|y| y.to_string())),
        ("stagione", number(parsed.season)),
        ("episodio", number(parsed.episode)),
        ("qualità", parsed.quality.clone()),
        ("gruppo", parsed.group.clone()),
        ("estensione", parsed.extension.clone()),
    ];
    for (label, value) in fields {
        writeln!(out, "{label:<11}{}", value.as_deref().unwrap_or("—"))?;
    }
    Ok(())
}

fn status_label(status: PreviewStatus) -> (&'static str, Option<&'static str>) {
    match status {
        PreviewStatus::Ready => (text::READY, None),
        PreviewStatus::Unchanged => (text::UNCHANGED, None),
        PreviewStatus::TargetExists => (text::CONFLICT, Some(text::TARGET_EXISTS)),
        PreviewStatus::DuplicateTarget => (text::CONFLICT, Some(text::DUPLICATE_TARGET)),
        PreviewStatus::SourceMissing => (text::ERROR, Some(text::SOURCE_MISSING)),
        PreviewStatus::InvalidTarget => (text::ERROR, Some(text::INVALID_TARGET)),
    }
}

/// Una riga dell'anteprima: stato e nome originale, poi il nuovo nome e l'eventuale nota.
fn print_row(
    out: &mut dyn Write,
    label: &str,
    name: &str,
    target: Option<&str>,
    note: Option<&str>,
) -> std::io::Result<()> {
    writeln!(out, "{:<22}{name}", format!("[{label}]"))?;
    if let Some(target) = target {
        writeln!(out, "{:<20}{} {target}", "", text::ARROW)?;
    }
    if let Some(note) = note {
        writeln!(out, "{:<22}({note})", "")?;
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Nuovo percorso relativo alla cartella del file originale.
fn relative_to_source(item: &RenameItem) -> String {
    let base = item.source.parent().unwrap_or(&item.target);
    let relative = item.target.strip_prefix(base).unwrap_or(&item.target);
    relative.display().to_string()
}

fn title_with_year(result: &providers::SearchResult) -> String {
    match result.year {
        Some(year) => format!("{} ({year})", result.title),
        None => result.title.clone(),
    }
}

#[cfg(test)]
mod tests;
