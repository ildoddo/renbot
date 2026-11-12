//! Viste dell'applicazione e stato condiviso tra di esse.

pub mod episodes;
pub mod library;
pub mod lists;
pub mod settings;

use std::path::PathBuf;

use renbot_core::config::Settings;
use renbot_core::providers::{Episode, SearchResult};
use renbot_core::renamer::PreviewRow;

use crate::backend::{Backend, Origin};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum View {
    Library,
    Episodes,
    Lists,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatusKind {
    Info,
    Success,
    Error,
}

/// Richiesta di una vista all'applicazione.
pub enum Request {
    Go(View),
    /// Apre la sezione Liste con una serie già scelta.
    NewList {
        series: SearchResult,
        episodes: Vec<Episode>,
    },
    /// Chiede conferma e poi rinomina le righe indicate.
    ConfirmRename {
        origin: Origin,
        rows: Vec<PreviewRow>,
    },
    Undo,
}

/// Ciò che ogni vista riceve dall'applicazione a ogni fotogramma.
pub struct Shared<'a> {
    pub settings: &'a mut Settings,
    pub backend: &'a Backend,
    pub status: &'a mut Option<(StatusKind, String)>,
    pub requests: &'a mut Vec<Request>,
    /// Vero se esiste un'operazione annullabile.
    pub can_undo: bool,
    /// Vero mentre una rinomina o un annullamento sono in corso.
    pub busy: bool,
}

impl Shared<'_> {
    pub fn info(&mut self, text: impl Into<String>) {
        *self.status = Some((StatusKind::Info, text.into()));
    }

    pub fn success(&mut self, text: impl Into<String>) {
        *self.status = Some((StatusKind::Success, text.into()));
    }

    pub fn error(&mut self, text: impl Into<String>) {
        *self.status = Some((StatusKind::Error, text.into()));
    }
}

/// Finestra di scelta dei file video.
pub fn pick_files() -> Option<Vec<PathBuf>> {
    rfd::FileDialog::new()
        .add_filter(
            crate::strings::VIDEO_FILES,
            &renbot_core::scanner::VIDEO_EXTENSIONS,
        )
        .pick_files()
}

pub fn pick_folder() -> Option<Vec<PathBuf>> {
    rfd::FileDialog::new()
        .pick_folder()
        .map(|folder| vec![folder])
}

/// Nome del file senza cartelle, per la visualizzazione.
pub fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// Titolo con anno tra parentesi, se noto.
pub fn title_with_year(result: &SearchResult) -> String {
    match result.year {
        Some(year) => format!("{} ({year})", result.title),
        None => result.title.clone(),
    }
}
