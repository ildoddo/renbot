//! Flusso completo su una cartella temporanea con un database simulato:
//! riconoscimento → ricerca → anteprima → rinomina → annullamento.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, Ordering};

use renbot_core::http::BoxFuture;
use renbot_core::matcher;
use renbot_core::parser::{self, MediaKind};
use renbot_core::providers::{
    self, Episode, MetadataProvider, ProviderError, ProviderId, SearchResult, DEFAULT_LANGUAGE,
};
use renbot_core::renamer::format::{render, FormatValues, DEFAULT_SERIES_FORMAT};
use renbot_core::renamer::{self, PreviewStatus, RenameItem};

/// Cartella temporanea eliminata alla fine del test.
struct TempDir(PathBuf);

impl TempDir {
    fn new() -> Self {
        static COUNTER: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "renbot-flow-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        );
        let path = std::env::temp_dir().join(name);
        fs::create_dir_all(&path).unwrap();
        Self(path)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Database simulato con una sola serie di tre episodi.
struct FakeProvider;

impl MetadataProvider for FakeProvider {
    fn id(&self) -> ProviderId {
        ProviderId::Tmdb
    }

    fn search<'a>(
        &'a self,
        kind: MediaKind,
        _query: &'a str,
        _year: Option<u16>,
        _language: &'a str,
    ) -> BoxFuture<'a, Result<Vec<SearchResult>, ProviderError>> {
        let series = move |id: &str, title: &str, year| SearchResult {
            provider: ProviderId::Tmdb,
            id: id.to_string(),
            kind,
            title: title.to_string(),
            original_title: None,
            year: Some(year),
        };
        Box::pin(async move {
            Ok(vec![
                series("2", "Show Name: Origins", 2022),
                series("1", "Show Name", 2019),
            ])
        })
    }

    fn episodes<'a>(
        &'a self,
        series_id: &'a str,
        _language: &'a str,
    ) -> BoxFuture<'a, Result<Vec<Episode>, ProviderError>> {
        assert_eq!(
            series_id, "1",
            "deve essere scelta la serie più somigliante"
        );
        let episode = |number, title: &str| Episode {
            season: 1,
            number,
            title: title.to_string(),
            air_date: None,
        };
        Box::pin(async move {
            Ok(vec![
                episode(1, "Pilota"),
                episode(2, "Il ritorno: parte 1"),
                episode(3, "Finale"),
            ])
        })
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

/// Costruisce la rinomina di un file come farà l'applicazione.
async fn plan(provider: &dyn MetadataProvider, file: &Path) -> RenameItem {
    let name = file.file_name().unwrap().to_string_lossy();
    let parsed = parser::parse(&name);
    let (_, found) = providers::search(
        provider,
        None,
        parsed.kind(),
        &parsed.title,
        parsed.year,
        DEFAULT_LANGUAGE,
    )
    .await
    .unwrap();
    let best = matcher::rank(&parsed.title, parsed.year, found).remove(0);
    assert!(!best.needs_review());

    let episodes = providers::episodes_localized(provider, &best.result.id, DEFAULT_LANGUAGE)
        .await
        .unwrap();
    let episode = episodes
        .iter()
        .find(|e| Some(e.season) == parsed.season && Some(e.number) == parsed.episode)
        .unwrap();
    let values = FormatValues::episode(&parsed, &best.result, episode);
    RenameItem::new(file, None, &render(DEFAULT_SERIES_FORMAT, &values))
}

fn files_in(dir: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![dir.to_path_buf()];
    while let Some(current) = pending.pop() {
        for entry in fs::read_dir(current).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                pending.push(path);
            } else {
                found.push(path.strip_prefix(dir).unwrap().to_path_buf());
            }
        }
    }
    found.sort();
    found
}

#[tokio::test]
async fn anteprima_rinomina_e_annullamento() {
    let dir = TempDir::new();
    let videos = dir.0.join("video");
    let log = dir.0.join("stato").join("undo.json");
    let originals = [
        "Show.Name.S01E01.1080p.WEB-DL.x264-GRP.mkv",
        "show name - 1x02.mkv",
        "Show.Name.103.720p.HDTV.mkv",
    ];
    fs::create_dir_all(&videos).unwrap();
    for name in originals {
        fs::write(videos.join(name), name).unwrap();
    }
    let original_files = files_in(&videos);

    // Anteprima: tutti i file sono pronti e il disco non cambia.
    let mut items = Vec::new();
    for name in originals {
        items.push(plan(&FakeProvider, &videos.join(name)).await);
    }
    let rows = renamer::preview(items);
    assert!(rows.iter().all(|row| row.status == PreviewStatus::Ready));
    assert_eq!(files_in(&videos), original_files);
    assert!(!renamer::can_undo(&log));

    // Rinomina.
    let report = renamer::apply(&rows, &log).unwrap();
    assert_eq!(report.renamed.len(), 3);
    assert!(report.failed.is_empty());
    let season = Path::new("Show Name").join("Season 1");
    assert_eq!(
        files_in(&videos),
        [
            season.join("Show Name - S01E01 - Pilota.mkv"),
            season.join("Show Name - S01E02 - Il ritorno parte 1.mkv"),
            season.join("Show Name - S01E03 - Finale.mkv"),
        ]
    );
    assert_eq!(
        fs::read_to_string(
            videos
                .join(&season)
                .join("Show Name - S01E02 - Il ritorno parte 1.mkv")
        )
        .unwrap(),
        "show name - 1x02.mkv",
        "il contenuto segue il file"
    );
    assert!(renamer::can_undo(&log));

    // Annullamento: nomi originali ripristinati e cartelle create rimosse.
    let report = renamer::undo(&log).unwrap();
    assert_eq!(report.restored.len(), 3);
    assert!(report.failed.is_empty());
    assert_eq!(files_in(&videos), original_files);
    assert!(!videos.join("Show Name").exists());
    for name in originals {
        assert_eq!(fs::read_to_string(videos.join(name)).unwrap(), name);
    }
    assert!(!renamer::can_undo(&log));
}
