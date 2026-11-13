//! Libreria: abbinamento automatico dei file, anteprima e rinomina.

use std::path::PathBuf;

use eframe::egui::{self, RichText, Ui};
use egui_phosphor::regular as icons;
use renbot_core::matcher::ScoredResult;
use renbot_core::parser::{self, MediaKind, ParsedName};
use renbot_core::providers::{Episode, ProviderId};
use renbot_core::renamer::{self, ApplyReport, PreviewRow, PreviewStatus, RenameItem, UndoReport};
use renbot_core::scanner;
use renbot_core::workflow;

use super::{file_name, pick_files, pick_folder, title_with_year, Request, Shared, View};
use crate::backend::{MatchData, Origin};
use crate::strings;
use crate::style::{self, Badge};

enum State {
    Pending,
    Matched {
        candidates: Vec<ScoredResult>,
        chosen: usize,
        episodes: Vec<Episode>,
    },
    NoMatch,
    Failed(String),
}

struct Row {
    id: u64,
    path: PathBuf,
    parsed: ParsedName,
    checked: bool,
    state: State,
    /// Destinazione e stato calcolati dall'ultima anteprima.
    preview: Option<PreviewRow>,
    renamed: bool,
}

impl Row {
    fn chosen(&self) -> Option<&ScoredResult> {
        match &self.state {
            State::Matched {
                candidates, chosen, ..
            } => candidates.get(*chosen),
            _ => None,
        }
    }

    /// Percorso relativo del nuovo nome, se il file è abbinato a un film o a un episodio.
    fn relative_target(&self, movie_format: &str, series_format: &str) -> Option<PathBuf> {
        let State::Matched { episodes, .. } = &self.state else {
            return None;
        };
        let result = &self.chosen()?.result;
        workflow::relative_target(&self.parsed, result, episodes, movie_format, series_format)
    }

    fn ready(&self) -> bool {
        !self.renamed
            && self
                .preview
                .as_ref()
                .is_some_and(|p| p.status == PreviewStatus::Ready)
    }

    fn needs_review(&self) -> bool {
        self.chosen().is_some_and(ScoredResult::needs_review)
    }

    fn badge(&self) -> Badge {
        if self.renamed {
            return Badge::new(icons::CHECK_FAT, strings::ST_RENAMED, Some(style::OK));
        }
        match &self.state {
            State::Pending => Badge::new(icons::HOURGLASS, strings::ST_PENDING, None),
            State::NoMatch => {
                Badge::new(icons::WARNING, strings::ST_NO_MATCH, Some(style::WARNING))
                    .detail(strings::DETAIL_NO_MATCH)
            }
            State::Failed(error) => {
                Badge::new(icons::X_CIRCLE, strings::ST_ERROR, Some(style::ERROR)).detail(error)
            }
            State::Matched { .. } => match self.preview.as_ref().map(|p| p.status) {
                None => Badge::new(icons::WARNING, strings::ST_NO_EPISODE, Some(style::WARNING))
                    .detail(strings::DETAIL_NO_EPISODE),
                Some(PreviewStatus::Ready) if self.needs_review() => {
                    Badge::new(icons::WARNING, strings::ST_REVIEW, Some(style::WARNING))
                        .detail(strings::DETAIL_REVIEW)
                }
                Some(PreviewStatus::Ready) => {
                    Badge::new(icons::CHECK_CIRCLE, strings::ST_READY, Some(style::OK))
                }
                Some(PreviewStatus::Unchanged) => {
                    Badge::new(icons::MINUS_CIRCLE, strings::ST_UNCHANGED, None)
                }
                Some(PreviewStatus::TargetExists) => {
                    Badge::new(icons::PROHIBIT, strings::ST_CONFLICT, Some(style::ERROR))
                        .detail(strings::DETAIL_TARGET_EXISTS)
                }
                Some(PreviewStatus::DuplicateTarget) => {
                    Badge::new(icons::PROHIBIT, strings::ST_CONFLICT, Some(style::ERROR))
                        .detail(strings::DETAIL_DUPLICATE)
                }
                Some(PreviewStatus::SourceMissing) => {
                    Badge::new(icons::X_CIRCLE, strings::ST_ERROR, Some(style::ERROR))
                        .detail(strings::DETAIL_SOURCE_MISSING)
                }
                Some(PreviewStatus::InvalidTarget) => {
                    Badge::new(icons::X_CIRCLE, strings::ST_ERROR, Some(style::ERROR))
                        .detail(strings::DETAIL_INVALID_TARGET)
                }
            },
        }
    }
}

/// Pannello "Corrispondenza" della riga selezionata.
#[derive(Default)]
struct Detail {
    query: String,
    searching: bool,
    /// Risultati di una ricerca manuale; in loro assenza valgono i candidati della riga.
    manual: Option<Result<Vec<ScoredResult>, String>>,
}

#[derive(Default)]
pub struct LibraryView {
    rows: Vec<Row>,
    next_id: u64,
    /// Cambia a ogni nuova ricerca globale: i risultati di quelle precedenti sono ignorati.
    generation: u64,
    selected: Option<u64>,
    detail: Detail,
    /// Vero quando l'anteprima va ricalcolata.
    dirty: bool,
}

impl LibraryView {
    pub fn add_paths(&mut self, paths: Vec<PathBuf>, shared: &mut Shared) {
        let files: Vec<PathBuf> = scanner::scan(&paths)
            .into_iter()
            .filter(|file| !self.rows.iter().any(|row| &row.path == file))
            .collect();
        if files.is_empty() {
            shared.info(strings::MSG_NO_VIDEO);
            return;
        }
        shared.info(strings::msg_added(files.len()));
        for path in files {
            let parsed = parser::parse(&file_name(&path));
            let id = self.next_id;
            self.next_id += 1;
            shared
                .backend
                .match_file(shared.settings, id, self.generation, parsed.clone());
            self.rows.push(Row {
                id,
                path,
                parsed,
                checked: true,
                state: State::Pending,
                preview: None,
                renamed: false,
            });
        }
    }

    /// Ripete la ricerca per tutti i file non ancora rinominati.
    fn rematch(&mut self, shared: &mut Shared) {
        self.generation += 1;
        self.selected = None;
        for row in self.rows.iter_mut().filter(|row| !row.renamed) {
            row.state = State::Pending;
            row.preview = None;
            row.checked = true;
            shared
                .backend
                .match_file(shared.settings, row.id, self.generation, row.parsed.clone());
        }
    }

    /// File in attesa di risposta e file totali, per la barra di avanzamento.
    pub fn progress(&self) -> Option<(usize, usize)> {
        let pending = self
            .rows
            .iter()
            .filter(|row| matches!(row.state, State::Pending))
            .count();
        (pending > 0).then(|| (self.rows.len() - pending, self.rows.len()))
    }

    pub fn on_matched(&mut self, id: u64, generation: u64, result: Result<MatchData, String>) {
        if generation != self.generation {
            return;
        }
        let Some(row) = self.rows.iter_mut().find(|row| row.id == id) else {
            return;
        };
        row.state = match result {
            Ok(data) if data.candidates.is_empty() => State::NoMatch,
            Ok(data) => State::Matched {
                candidates: data.candidates,
                chosen: 0,
                episodes: data.episodes,
            },
            Err(error) => State::Failed(error),
        };
        // Gli abbinamenti dubbi vanno inclusi a mano.
        row.checked = !row.needs_review();
        self.dirty = true;
    }

    pub fn on_manual_results(&mut self, id: u64, result: Result<Vec<ScoredResult>, String>) {
        if self.selected == Some(id) {
            self.detail.searching = false;
            self.detail.manual = Some(result);
        }
    }

    pub fn on_resolved(
        &mut self,
        ids: &[u64],
        choice: ScoredResult,
        result: Result<Vec<Episode>, String>,
    ) {
        for row in self.rows.iter_mut().filter(|row| ids.contains(&row.id)) {
            row.state = match &result {
                Ok(episodes) => State::Matched {
                    candidates: vec![choice.clone()],
                    chosen: 0,
                    episodes: episodes.clone(),
                },
                Err(error) => State::Failed(error.clone()),
            };
            row.checked = true;
        }
        self.dirty = true;
    }

    pub fn on_applied(&mut self, report: &ApplyReport) {
        for row in &mut self.rows {
            if report.renamed.iter().any(|item| item.source == row.path) {
                row.renamed = true;
            }
            if let Some((_, error)) = report
                .failed
                .iter()
                .find(|(item, _)| item.source == row.path)
            {
                row.state = State::Failed(error.to_string());
            }
        }
        self.dirty = true;
    }

    pub fn on_undone(&mut self, report: &UndoReport) {
        for row in &mut self.rows {
            if report.restored.iter().any(|item| item.source == row.path) {
                row.renamed = false;
            }
        }
        self.dirty = true;
    }

    fn refresh_preview(&mut self, shared: &Shared) {
        self.dirty = false;
        let settings = &shared.settings;
        let mut indexes = Vec::new();
        let mut items = Vec::new();
        for (index, row) in self.rows.iter_mut().enumerate() {
            if row.renamed {
                continue;
            }
            row.preview = None;
            if let Some(relative) =
                row.relative_target(&settings.movie_format, &settings.series_format)
            {
                indexes.push(index);
                items.push(RenameItem::new(&row.path, None, &relative));
            }
        }
        for (index, preview) in indexes.into_iter().zip(renamer::preview(items)) {
            self.rows[index].preview = Some(preview);
        }
    }

    /// Chiede di ricalcolare l'anteprima, ad esempio dopo un cambio di formato.
    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn selected_rows(&self) -> Vec<PreviewRow> {
        self.rows
            .iter()
            .filter(|row| row.checked && row.ready())
            .filter_map(|row| row.preview.clone())
            .collect()
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared) {
        if self.dirty {
            self.refresh_preview(shared);
        }

        egui::Panel::top("library_bar").show(ui, |ui| {
            ui.add_space(style::UNIT);
            self.toolbar(ui, shared);
            if !shared.backend.has_key(shared.settings.provider) {
                style::banner(ui, style::WARNING, |ui| {
                    ui.label(style::icon_text(icons::KEY, strings::LIB_NO_KEY));
                    if ui.button(strings::OPEN_SETTINGS).clicked() {
                        shared.requests.push(Request::Go(View::Settings));
                    }
                });
            }
            ui.add_space(style::UNIT);
        });

        egui::Panel::bottom("library_actions").show(ui, |ui| {
            ui.add_space(style::UNIT);
            self.actions(ui, shared);
            ui.add_space(style::UNIT);
        });

        if self.selected.is_some() {
            egui::Panel::right("library_detail")
                .default_size(320.0)
                .min_size(260.0)
                .show(ui, |ui| self.detail_panel(ui, shared));
        }

        egui::CentralPanel::default().show(ui, |ui| {
            if self.rows.is_empty() {
                style::empty_state(
                    ui,
                    icons::TRAY_ARROW_DOWN,
                    strings::LIB_EMPTY_TITLE,
                    strings::LIB_EMPTY_HINT,
                );
            } else {
                self.table(ui);
            }
        });

        // Canc esclude la riga selezionata, se non si sta scrivendo in un campo.
        let delete = ui.input(|i| i.key_pressed(egui::Key::Delete));
        if delete && !ui.ctx().egui_wants_keyboard_input() {
            if let Some(row) = self.rows.iter_mut().find(|r| Some(r.id) == self.selected) {
                row.checked = false;
            }
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, shared: &mut Shared) {
        ui.horizontal_wrapped(|ui| {
            if ui
                .button(style::icon_text(icons::FILE_PLUS, strings::ADD_FILES))
                .clicked()
            {
                if let Some(paths) = pick_files() {
                    self.add_paths(paths, shared);
                }
            }
            if ui
                .button(style::icon_text(icons::FOLDER_PLUS, strings::ADD_FOLDER))
                .clicked()
            {
                if let Some(paths) = pick_folder() {
                    self.add_paths(paths, shared);
                }
            }
            ui.separator();

            let before = (shared.settings.provider, shared.settings.use_fallback);
            ui.label(strings::DATABASE);
            for id in [ProviderId::Tmdb, ProviderId::Tvdb] {
                ui.selectable_value(&mut shared.settings.provider, id, strings::provider(id));
            }
            ui.checkbox(&mut shared.settings.use_fallback, strings::USE_FALLBACK);
            if before != (shared.settings.provider, shared.settings.use_fallback) {
                self.rematch(shared);
            }
            ui.separator();

            let has_rows = !self.rows.is_empty();
            let rematch = egui::Button::new(style::icon_text(
                icons::ARROWS_CLOCKWISE,
                strings::LIB_REMATCH,
            ));
            if ui.add_enabled(has_rows, rematch).clicked() {
                self.rematch(shared);
            }
            let clear = egui::Button::new(style::icon_text(icons::BROOM, strings::CLEAR));
            if ui.add_enabled(has_rows, clear).clicked() {
                self.rows.clear();
                self.selected = None;
                self.generation += 1;
            }
        });
    }

    fn actions(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let ready = self.rows.iter().filter(|r| r.ready() && !r.needs_review());
        let ready = ready.count();
        let review = self.rows.iter().filter(|r| r.ready() && r.needs_review());
        let review = review.count();
        let conflicts = self
            .rows
            .iter()
            .filter(|row| {
                row.preview.as_ref().is_some_and(|p| {
                    matches!(
                        p.status,
                        PreviewStatus::TargetExists | PreviewStatus::DuplicateTarget
                    )
                })
            })
            .count();
        let selected = self.selected_rows();

        ui.horizontal(|ui| {
            ui.label(strings::library_summary(
                self.rows.len(),
                ready,
                review,
                conflicts,
            ));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = !selected.is_empty() && !shared.busy;
                let label = strings::rename_n(selected.len());
                if style::primary_button(ui, enabled, icons::PLAY, &label).clicked() {
                    shared.requests.push(Request::ConfirmRename {
                        origin: Origin::Library,
                        rows: selected,
                    });
                }
                let undo = egui::Button::new(style::icon_text(
                    icons::ARROW_COUNTER_CLOCKWISE,
                    strings::UNDO_LAST,
                ));
                if ui
                    .add_enabled(shared.can_undo && !shared.busy, undo)
                    .clicked()
                {
                    shared.requests.push(Request::Undo);
                }
            });
        });
    }

    fn table(&mut self, ui: &mut Ui) {
        let spacing = ui.spacing().item_spacing.x;
        let check = 20.0;
        let status = 150.0;
        let flexible = (ui.available_width() - check - status - 5.0 * spacing - 16.0).max(300.0);
        let (original, new, matched) = (flexible * 0.34, flexible * 0.40, flexible * 0.26);
        let mut select = None;

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("library_table")
                    .striped(true)
                    .min_row_height(style::ROW_HEIGHT)
                    .show(ui, |ui| {
                        ui.label("");
                        style::header(ui, status, strings::COL_STATUS);
                        style::header(ui, original, strings::COL_ORIGINAL);
                        style::header(ui, new, strings::COL_NEW);
                        style::header(ui, matched, strings::COL_MATCH);
                        ui.end_row();

                        for row in &mut self.rows {
                            ui.add_enabled(
                                !row.renamed,
                                egui::Checkbox::without_text(&mut row.checked),
                            );
                            row.badge().show(ui, status);
                            style::cell(ui, original, &file_name(&row.path));

                            let target = row.preview.as_ref().map(|p| {
                                let base = row.path.parent().unwrap_or(&p.item.target);
                                let relative = p.item.target.strip_prefix(base);
                                relative.unwrap_or(&p.item.target).display().to_string()
                            });
                            style::cell(ui, new, target.as_deref().unwrap_or(strings::LIST_NONE));

                            let label = match row.chosen() {
                                Some(scored) => format!(
                                    "{}  ·  {:.0}%",
                                    title_with_year(&scored.result),
                                    scored.score * 100.0
                                ),
                                None => row.parsed.title.clone(),
                            };
                            let selected = self.selected == Some(row.id);
                            let response = ui
                                .allocate_ui(egui::vec2(matched, style::ROW_HEIGHT), |ui| {
                                    ui.set_min_width(matched);
                                    ui.add(egui::Button::selectable(selected, label).truncate())
                                })
                                .inner;
                            if response.clicked() {
                                select = Some(row.id);
                            }
                            ui.end_row();
                        }
                    });
            });

        if let Some(id) = select {
            if self.selected == Some(id) {
                self.selected = None;
            } else {
                self.selected = Some(id);
                let title = self
                    .rows
                    .iter()
                    .find(|r| r.id == id)
                    .map(|r| r.parsed.title.clone());
                self.detail = Detail {
                    query: title.unwrap_or_default(),
                    ..Detail::default()
                };
            }
        }
    }

    fn detail_panel(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let Some(index) = self.rows.iter().position(|r| Some(r.id) == self.selected) else {
            self.selected = None;
            return;
        };
        let mut close = false;
        let mut search = false;
        let mut pick: Option<ScoredResult> = None;

        ui.add_space(style::UNIT);
        ui.horizontal(|ui| {
            ui.heading(strings::LIB_PANEL);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                close = ui.button(icons::X).on_hover_text(strings::CLOSE).clicked();
            });
        });
        ui.separator();

        let row = &self.rows[index];
        let parsed = &row.parsed;
        ui.label(RichText::new(strings::LIB_FROM_NAME).strong());
        egui::Grid::new("detail_parsed")
            .num_columns(2)
            .show(ui, |ui| {
                let kind = match parsed.kind() {
                    MediaKind::Movie => strings::KIND_MOVIE,
                    MediaKind::Episode => strings::KIND_SERIES,
                };
                let episode = match (parsed.season, parsed.episode) {
                    (Some(s), Some(e)) => Some(format!("S{s:02}E{e:02}")),
                    (None, Some(e)) => Some(format!("E{e:02}")),
                    _ => None,
                };
                let fields = [
                    (strings::F_TITLE, Some(format!("{} ({kind})", parsed.title))),
                    (strings::F_YEAR, parsed.year.map(|y| y.to_string())),
                    (strings::F_EPISODE, episode),
                    (strings::F_QUALITY, parsed.quality.clone()),
                    (strings::F_GROUP, parsed.group.clone()),
                ];
                for (name, value) in fields {
                    if let Some(value) = value {
                        ui.weak(name);
                        ui.add(egui::Label::new(value).wrap());
                        ui.end_row();
                    }
                }
            });
        ui.separator();

        ui.horizontal(|ui| {
            let button = egui::Button::new(icons::MAGNIFYING_GLASS);
            let width = ui.available_width() - 48.0;
            let field = egui::TextEdit::singleline(&mut self.detail.query)
                .hint_text(strings::LIB_SEARCH_HINT)
                .desired_width(width);
            let response = ui.add(field);
            let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let clicked = ui.add(button).on_hover_text(strings::SEARCH).clicked();
            search = (enter || clicked) && !self.detail.query.trim().is_empty();
        });

        ui.label(RichText::new(strings::LIB_CANDIDATES).strong());
        if self.detail.searching {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(strings::LOADING);
            });
        }
        let current = row
            .chosen()
            .map(|c| (c.result.provider, c.result.id.clone()));
        let own: &[ScoredResult] = match &row.state {
            State::Matched { candidates, .. } => candidates,
            _ => &[],
        };
        let candidates: &[ScoredResult] = match &self.detail.manual {
            Some(Ok(found)) => found,
            Some(Err(error)) => {
                ui.colored_label(style::ERROR, error);
                &[]
            }
            None => own,
        };
        if candidates.is_empty() && !self.detail.searching {
            ui.weak(strings::LIB_NO_CANDIDATES);
        }
        egui::ScrollArea::vertical()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                for scored in candidates {
                    let result = &scored.result;
                    let in_use = current == Some((result.provider, result.id.clone()));
                    egui::Frame::group(ui.style()).show(ui, |ui| {
                        ui.set_width(ui.available_width());
                        ui.add(
                            egui::Label::new(RichText::new(title_with_year(result)).strong())
                                .wrap(),
                        );
                        if let Some(original) = &result.original_title {
                            ui.weak(original);
                        }
                        ui.horizontal(|ui| {
                            ui.weak(format!(
                                "{} · {:.0}%",
                                strings::provider(result.provider),
                                scored.score * 100.0
                            ));
                            ui.with_layout(
                                egui::Layout::right_to_left(egui::Align::Center),
                                |ui| {
                                    if in_use {
                                        ui.colored_label(style::OK, strings::LIB_IN_USE);
                                    } else if ui.button(strings::LIB_USE_THIS).clicked() {
                                        pick = Some(scored.clone());
                                    }
                                },
                            );
                        });
                    });
                }
            });

        if close {
            self.selected = None;
        }
        if search {
            self.detail.searching = true;
            self.detail.manual = None;
            let row = &self.rows[index];
            shared.backend.manual_search(
                shared.settings,
                row.id,
                self.detail.query.clone(),
                row.parsed.clone(),
            );
        }
        if let Some(choice) = pick {
            // La scelta vale per tutti i file con lo stesso titolo ricavato dal nome.
            let (title, kind) = {
                let parsed = &self.rows[index].parsed;
                (parsed.title.to_lowercase(), parsed.kind())
            };
            let ids: Vec<u64> = self
                .rows
                .iter_mut()
                .filter(|r| !r.renamed && r.parsed.kind() == kind)
                .filter(|r| r.parsed.title.to_lowercase() == title)
                .map(|r| {
                    r.state = State::Pending;
                    r.preview = None;
                    r.id
                })
                .collect();
            shared.backend.resolve(shared.settings, ids, choice);
        }
    }
}
