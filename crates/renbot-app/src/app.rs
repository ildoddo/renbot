//! Finestra principale: barra laterale, viste, barra di stato e conferma delle rinomine.

use std::path::PathBuf;
use std::sync::mpsc::Receiver;

use eframe::egui::{self, RichText, Ui};
use egui_phosphor::regular as icons;
use renbot_core::config::Settings;
use renbot_core::renamer::{self, PreviewRow};

use crate::backend::{Backend, Event, Origin};
use crate::strings;
use crate::style;
use crate::views::episodes::EpisodesView;
use crate::views::library::LibraryView;
use crate::views::lists::ListsView;
use crate::views::settings::SettingsView;
use crate::views::{file_name, Request, Shared, StatusKind, View};

/// Rinomina in attesa di conferma.
struct Pending {
    origin: Origin,
    rows: Vec<PreviewRow>,
}

pub struct RenbotApp {
    settings: Settings,
    saved: Settings,
    settings_path: Option<PathBuf>,
    undo_log: Option<PathBuf>,
    backend: Backend,
    events: Receiver<Event>,
    view: View,
    library: LibraryView,
    episodes: EpisodesView,
    lists: ListsView,
    settings_view: SettingsView,
    status: Option<(StatusKind, String)>,
    requests: Vec<Request>,
    confirm: Option<Pending>,
    can_undo: bool,
    busy: bool,
    /// Ultimo errore già scritto nel registro.
    logged_error: Option<String>,
}

/// Costruisce lo stato condiviso prendendo in prestito i soli campi necessari,
/// così le viste restano modificabili nello stesso momento.
macro_rules! shared {
    ($app:expr) => {
        Shared {
            settings: &mut $app.settings,
            backend: &$app.backend,
            status: &mut $app.status,
            requests: &mut $app.requests,
            can_undo: $app.can_undo,
            busy: $app.busy,
        }
    };
}

impl RenbotApp {
    pub fn new(cc: &eframe::CreationContext<'_>) -> anyhow::Result<Self> {
        style::install(&cc.egui_ctx);
        let settings_path = Settings::default_path();
        let settings = settings_path
            .as_deref()
            .map(Settings::load)
            .unwrap_or_default();
        style::apply_theme(&cc.egui_ctx, settings.theme);

        let (backend, events) = Backend::new(cc.egui_ctx.clone())?;
        let undo_log = renamer::default_undo_log();
        let can_undo = undo_log.as_deref().is_some_and(renamer::can_undo);

        let mut app = Self {
            saved: settings.clone(),
            settings,
            settings_path,
            undo_log,
            backend,
            events,
            view: View::Library,
            library: LibraryView::default(),
            episodes: EpisodesView::default(),
            lists: ListsView::default(),
            settings_view: SettingsView::placeholder(),
            status: None,
            requests: Vec::new(),
            confirm: None,
            can_undo,
            busy: false,
            logged_error: None,
        };
        app.settings_view = SettingsView::new(&shared!(app));
        Ok(app)
    }

    fn handle_events(&mut self) {
        while let Ok(event) = self.events.try_recv() {
            match event {
                Event::Matched {
                    id,
                    generation,
                    result,
                } => {
                    self.library.on_matched(id, generation, result);
                }
                Event::ManualResults { id, result } => self.library.on_manual_results(id, result),
                Event::Resolved {
                    ids,
                    choice,
                    result,
                } => {
                    self.library.on_resolved(&ids, choice, result);
                }
                Event::SeriesFound { origin, result } => match origin {
                    Origin::Lists => self.lists.on_found(result),
                    _ => self.episodes.on_found(result),
                },
                Event::EpisodesLoaded {
                    origin,
                    series,
                    result,
                } => match origin {
                    Origin::Lists => self.lists.on_loaded(series, result, &mut shared!(self)),
                    _ => self.episodes.on_loaded(series, result),
                },
                Event::KeyVerified { key, result } => self.settings_view.on_verified(key, result),
                Event::Applied { origin, result } => {
                    self.busy = false;
                    match result {
                        Ok(report) => {
                            match origin {
                                Origin::Lists => self.lists.on_applied(&report),
                                _ => self.library.on_applied(&report),
                            }
                            let message =
                                strings::msg_renamed(report.renamed.len(), report.failed.len());
                            let kind = if report.failed.is_empty() {
                                StatusKind::Success
                            } else {
                                StatusKind::Error
                            };
                            self.status = Some((kind, message));
                        }
                        Err(error) => {
                            self.status = Some((StatusKind::Error, strings::msg_error(error)))
                        }
                    }
                    self.refresh_undo();
                }
                Event::Undone { result } => {
                    self.busy = false;
                    match result {
                        Ok(report) => {
                            self.library.on_undone(&report);
                            self.lists.on_undone(&report);
                            let message =
                                strings::msg_undone(report.restored.len(), report.failed.len());
                            let kind = if report.failed.is_empty() {
                                StatusKind::Success
                            } else {
                                StatusKind::Error
                            };
                            self.status = Some((kind, message));
                        }
                        Err(error) => {
                            self.status = Some((StatusKind::Error, strings::msg_error(error)))
                        }
                    }
                    self.refresh_undo();
                }
            }
        }
    }

    /// Riporta nel registro ogni nuovo errore mostrato nella barra di stato.
    fn log_new_error(&mut self) {
        if let Some((StatusKind::Error, text)) = &self.status {
            if self.logged_error.as_deref() != Some(text) {
                tracing::error!("{text}");
                self.logged_error = Some(text.clone());
            }
        }
    }

    fn refresh_undo(&mut self) {
        self.can_undo = self.undo_log.as_deref().is_some_and(renamer::can_undo);
    }

    fn handle_requests(&mut self) {
        for request in std::mem::take(&mut self.requests) {
            match request {
                Request::Go(view) => self.view = view,
                Request::NewList { series, episodes } => {
                    let language = self.settings.language.clone();
                    self.lists.open(series, &language, episodes);
                    self.view = View::Lists;
                }
                Request::ConfirmRename { origin, rows } => {
                    self.confirm = Some(Pending { origin, rows });
                }
                Request::Undo => match &self.undo_log {
                    Some(log) if self.can_undo => {
                        self.busy = true;
                        self.backend.undo(log.clone());
                    }
                    _ => {
                        self.status =
                            Some((StatusKind::Info, strings::MSG_NOTHING_TO_UNDO.to_string()))
                    }
                },
            }
        }
    }

    /// Salva le impostazioni modificate e ne applica gli effetti.
    fn sync_settings(&mut self, ctx: &egui::Context) {
        if self.settings == self.saved {
            return;
        }
        if self.settings.theme != self.saved.theme {
            style::apply_theme(ctx, self.settings.theme);
        }
        let formats = |s: &Settings| (s.movie_format.clone(), s.series_format.clone());
        if formats(&self.settings) != formats(&self.saved) {
            self.library.invalidate();
            self.lists.invalidate();
        }
        if let Some(path) = &self.settings_path {
            if let Err(error) = self.settings.save(path) {
                self.status = Some((StatusKind::Error, strings::msg_error(error)));
            }
        }
        self.saved = self.settings.clone();
    }

    fn handle_drops(&mut self, ctx: &egui::Context) {
        let dropped: Vec<PathBuf> = ctx.input(|input| {
            let files = input.raw.dropped_files.iter();
            files.map(|file| file.path().to_path_buf()).collect()
        });
        if dropped.is_empty() {
            return;
        }
        if self.view == View::Lists {
            self.lists.add_paths(dropped, &mut shared!(self));
        } else {
            self.view = View::Library;
            self.library.add_paths(dropped, &mut shared!(self));
        }
    }

    fn sidebar(&mut self, ui: &mut Ui) {
        ui.add_space(3.0 * style::UNIT);
        ui.label(RichText::new(strings::APP_NAME).size(22.0).strong());
        ui.add_space(3.0 * style::UNIT);
        let mut entry = |ui: &mut Ui, view: View, icon: &str, text: &str| {
            let label = RichText::new(style::icon_text(icon, text)).size(15.0);
            let button = egui::Button::selectable(self.view == view, label);
            if ui.add_sized([ui.available_width(), 34.0], button).clicked() {
                self.view = view;
            }
        };
        entry(ui, View::Library, icons::FOLDERS, strings::NAV_LIBRARY);
        entry(
            ui,
            View::Episodes,
            icons::TELEVISION_SIMPLE,
            strings::NAV_EPISODES,
        );
        entry(ui, View::Lists, icons::LIST_NUMBERS, strings::NAV_LISTS);
        ui.with_layout(egui::Layout::bottom_up(egui::Align::Min), |ui| {
            ui.add_space(2.0 * style::UNIT);
            entry(ui, View::Settings, icons::GEAR, strings::NAV_SETTINGS);
            ui.separator();
        });
    }

    fn status_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            if let Some((done, total)) = self.library.progress() {
                ui.spinner();
                ui.label(strings::msg_matching(done, total));
                let fraction = done as f32 / total.max(1) as f32;
                ui.add(egui::ProgressBar::new(fraction).desired_width(180.0));
                ui.separator();
            } else if self.busy {
                ui.spinner();
            }
            match &self.status {
                Some((kind, text)) => {
                    let (icon, color) = match kind {
                        StatusKind::Info => (icons::INFO, ui.visuals().text_color()),
                        StatusKind::Success => (icons::CHECK_CIRCLE, style::OK),
                        StatusKind::Error => (icons::X_CIRCLE, style::ERROR),
                    };
                    let label = RichText::new(style::icon_text(icon, text)).color(color);
                    ui.add(egui::Label::new(label).truncate())
                        .on_hover_text(text);
                }
                None => {
                    ui.label("");
                }
            }
        });
    }

    fn confirm_dialog(&mut self, ctx: &egui::Context) {
        let Some(pending) = &self.confirm else { return };
        let mut decision = None;
        egui::Modal::new(egui::Id::new("confirm_rename")).show(ctx, |ui| {
            ui.set_width(460.0);
            ui.heading(strings::CONFIRM_TITLE);
            ui.add_space(style::UNIT);
            ui.label(strings::confirm_body(pending.rows.len()));
            if let Some(first) = pending.rows.first() {
                ui.horizontal_wrapped(|ui| {
                    ui.weak(strings::CONFIRM_FIRST);
                    ui.label(file_name(&first.item.source));
                    ui.weak("→");
                    ui.label(first.item.target.display().to_string());
                });
            }
            ui.weak(strings::CONFIRM_NOTE);
            ui.add_space(2.0 * style::UNIT);
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let label = strings::rename_n(pending.rows.len());
                if style::primary_button(ui, true, icons::PLAY, &label).clicked() {
                    decision = Some(true);
                }
                if ui.button(strings::CANCEL).clicked() {
                    decision = Some(false);
                }
            });
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            decision = Some(false);
        }

        match (decision, &self.undo_log) {
            (Some(true), Some(log)) => {
                if let Some(pending) = self.confirm.take() {
                    self.busy = true;
                    self.backend
                        .apply(pending.origin, pending.rows, log.clone());
                }
            }
            (Some(_), _) => self.confirm = None,
            (None, _) => {}
        }
    }
}

impl eframe::App for RenbotApp {
    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.handle_events();
        self.handle_drops(&ctx);

        egui::Panel::left("sidebar")
            .exact_size(178.0)
            .resizable(false)
            .show(ui, |ui| self.sidebar(ui));
        egui::Panel::bottom("status_bar").show(ui, |ui| {
            ui.add_space(style::UNIT);
            self.status_bar(ui);
            ui.add_space(style::UNIT);
        });

        match self.view {
            View::Library => self.library.ui(ui, &mut shared!(self)),
            View::Episodes => self.episodes.ui(ui, &mut shared!(self)),
            View::Lists => self.lists.ui(ui, &mut shared!(self)),
            View::Settings => self.settings_view.ui(ui, &mut shared!(self)),
        }

        self.confirm_dialog(&ctx);
        self.handle_requests();
        self.sync_settings(&ctx);
        self.log_new_error();
    }
}
