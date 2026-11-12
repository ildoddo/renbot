//! Liste: assegnazione manuale degli episodi a più file di una serie.

use std::path::PathBuf;

use eframe::egui::{self, RichText, Ui};
use egui_phosphor::regular as icons;
use renbot_core::providers::{Episode, SearchResult};
use renbot_core::renamer::list::{EpisodeRef, RenameList};
use renbot_core::renamer::{self, ApplyReport, PreviewRow, PreviewStatus, UndoReport};
use renbot_core::scanner;

use super::episodes::SeriesSearch;
use super::{file_name, pick_files, title_with_year, Request, Shared};
use crate::backend::Origin;
use crate::strings;
use crate::style::{self, Badge};

#[derive(Default)]
pub struct ListsView {
    list: Option<RenameList>,
    search: SeriesSearch,
    /// Serie scelta, in attesa dell'elenco degli episodi.
    loading: Option<SearchResult>,
    season: u32,
    start: u32,
    selected: Option<usize>,
    /// File conservati mentre si sceglie un'altra serie.
    kept_files: Vec<PathBuf>,
    /// Anteprima per riga, allineata a `list.rows`.
    preview: Vec<Option<PreviewRow>>,
    dirty: bool,
}

enum RowAction {
    Up(usize),
    Down(usize),
    Select(usize),
}

impl ListsView {
    pub fn open(&mut self, series: SearchResult, language: &str, episodes: Vec<Episode>) {
        let mut list = RenameList::new(series, language, episodes);
        // I file già aggiunti restano quando si cambia serie.
        let mut files = std::mem::take(&mut self.kept_files);
        if let Some(old) = self.list.take() {
            files.extend(old.rows.into_iter().map(|row| row.file));
        }
        list.add_files(files);
        self.set_list(list);
    }

    fn set_list(&mut self, list: RenameList) {
        self.season = list.seasons().into_iter().find(|s| *s > 0).unwrap_or(1);
        self.start = 1;
        self.selected = None;
        self.loading = None;
        self.search = SeriesSearch::default();
        self.list = Some(list);
        self.dirty = true;
    }

    pub fn add_paths(&mut self, paths: Vec<PathBuf>, shared: &mut Shared) {
        let files = scanner::scan(&paths);
        match &mut self.list {
            Some(list) if !files.is_empty() => {
                shared.info(strings::msg_added(files.len()));
                list.add_files(files);
                self.dirty = true;
            }
            Some(_) => shared.info(strings::MSG_NO_VIDEO),
            None => shared.info(strings::LIST_EMPTY_HINT),
        }
    }

    pub fn on_found(&mut self, result: Result<Vec<SearchResult>, String>) {
        self.search.on_found(result);
    }

    pub fn on_loaded(
        &mut self,
        series: SearchResult,
        result: Result<Vec<Episode>, String>,
        shared: &mut Shared,
    ) {
        if self.loading.as_ref() != Some(&series) {
            return;
        }
        match result {
            Ok(episodes) => self.open(series, &shared.settings.language.clone(), episodes),
            Err(error) => {
                self.loading = None;
                shared.error(strings::msg_error(error));
            }
        }
    }

    pub fn on_applied(&mut self, report: &ApplyReport) {
        if let Some(list) = &mut self.list {
            for row in &mut list.rows {
                if let Some(item) = report.renamed.iter().find(|item| item.source == row.file) {
                    row.file = item.target.clone();
                }
            }
        }
        self.dirty = true;
    }

    pub fn on_undone(&mut self, report: &UndoReport) {
        if let Some(list) = &mut self.list {
            for row in &mut list.rows {
                if let Some(item) = report.restored.iter().find(|item| item.target == row.file) {
                    row.file = item.source.clone();
                }
            }
        }
        self.dirty = true;
    }

    pub fn invalidate(&mut self) {
        self.dirty = true;
    }

    fn refresh_preview(&mut self, shared: &Shared) {
        self.dirty = false;
        let Some(list) = &self.list else {
            self.preview.clear();
            return;
        };
        let format = &shared.settings.series_format;
        let items: Vec<(usize, _)> = (0..list.rows.len())
            .filter_map(|index| Some((index, list.row_item(index, format, None)?)))
            .collect();
        let mut preview = vec![None; list.rows.len()];
        let rows = renamer::preview(items.iter().map(|(_, item)| item.clone()).collect());
        for ((index, _), row) in items.iter().zip(rows) {
            preview[*index] = Some(row);
        }
        self.preview = preview;
    }

    fn ready_rows(&self) -> Vec<PreviewRow> {
        let duplicates = self.list.as_ref().map(RenameList::duplicate_rows);
        let duplicates = duplicates.unwrap_or_default();
        self.preview
            .iter()
            .enumerate()
            .filter(|(index, _)| !duplicates.contains(index))
            .filter_map(|(_, row)| row.clone())
            .filter(|row| row.status == PreviewStatus::Ready)
            .collect()
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared) {
        if self.dirty || self.preview.len() != self.list.as_ref().map_or(0, |l| l.rows.len()) {
            self.refresh_preview(shared);
        }

        egui::Panel::top("lists_bar").show(ui, |ui| {
            ui.add_space(style::UNIT);
            self.toolbar(ui, shared);
            ui.add_space(style::UNIT);
        });

        if self.list.is_none() {
            let mut picked = None;
            egui::CentralPanel::default().show(ui, |ui| {
                if let Some(series) = &self.loading {
                    ui.horizontal(|ui| {
                        ui.spinner();
                        ui.label(format!("{} {}", strings::LOADING, title_with_year(series)));
                    });
                } else if self.search.results.is_none() {
                    style::empty_state(
                        ui,
                        icons::LIST_NUMBERS,
                        strings::LIST_EMPTY_TITLE,
                        strings::LIST_EMPTY_HINT,
                    );
                } else {
                    ui.label(RichText::new(strings::EP_RESULTS).strong());
                    picked = self.search.results(ui, None);
                }
            });
            if let Some(series) = picked {
                shared
                    .backend
                    .load_episodes(shared.settings, Origin::Lists, series.clone());
                self.loading = Some(series);
            }
            return;
        }

        egui::Panel::bottom("lists_actions").show(ui, |ui| {
            ui.add_space(style::UNIT);
            self.actions(ui, shared);
            ui.add_space(style::UNIT);
        });
        egui::CentralPanel::default().show(ui, |ui| self.table(ui));

        let delete = ui.input(|i| i.key_pressed(egui::Key::Delete));
        if delete && !ui.ctx().egui_wants_keyboard_input() {
            if let (Some(list), Some(row)) = (&mut self.list, self.selected) {
                list.set_included(row, false);
                self.dirty = true;
            }
        }
    }

    fn toolbar(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let mut change = false;
        let mut load = false;
        let mut save = false;
        ui.horizontal_wrapped(|ui| {
            match &self.list {
                None => self.search.bar(ui, shared, Origin::Lists),
                Some(list) => {
                    ui.heading(title_with_year(&list.series));
                    ui.weak(format!(
                        "{} · {}",
                        strings::provider(list.series.provider),
                        strings::episode_count(list.episodes.len(), list.seasons().len())
                    ));
                    change = ui
                        .button(style::icon_text(icons::SWAP, strings::LIST_CHANGE))
                        .clicked();
                }
            }
            ui.separator();
            load = ui
                .button(style::icon_text(icons::FOLDER_OPEN, strings::LIST_OPEN))
                .clicked();
            let button =
                egui::Button::new(style::icon_text(icons::FLOPPY_DISK, strings::LIST_SAVE));
            save = ui.add_enabled(self.list.is_some(), button).clicked();
        });

        if let Some(list) = &mut self.list {
            ui.horizontal_wrapped(|ui| {
                if ui
                    .button(style::icon_text(icons::FILE_PLUS, strings::ADD_FILES))
                    .clicked()
                {
                    if let Some(files) = pick_files() {
                        list.add_files(scanner::scan(&files));
                        self.dirty = true;
                    }
                }
                ui.separator();
                ui.label(strings::SEASON);
                egui::ComboBox::from_id_salt("list_season")
                    .selected_text(self.season.to_string())
                    .width(56.0)
                    .show_ui(ui, |ui| {
                        for season in list.seasons() {
                            ui.selectable_value(&mut self.season, season, season.to_string());
                        }
                    });
                ui.label(strings::LIST_START);
                ui.add(egui::DragValue::new(&mut self.start).range(0..=9999));
                let auto = style::icon_text(icons::LIST_NUMBERS, strings::LIST_AUTO);
                if ui
                    .button(auto)
                    .on_hover_text(strings::LIST_AUTO_HINT)
                    .clicked()
                {
                    let free: Vec<usize> = (0..list.rows.len())
                        .filter(|i| list.rows[*i].episode.is_none())
                        .collect();
                    list.auto_assign(&free, self.season, self.start);
                    self.dirty = true;
                }
                let reset = style::icon_text(icons::ERASER, strings::LIST_RESET);
                if ui
                    .button(reset)
                    .on_hover_text(strings::LIST_RESET_HINT)
                    .clicked()
                {
                    for row in 0..list.rows.len() {
                        list.assign(row, None);
                    }
                    self.dirty = true;
                }
            });
        }

        if change {
            // I file restano: vengono ripresi quando si sceglie la nuova serie.
            let files: Vec<PathBuf> = self.list.take().map_or(Vec::new(), |list| {
                list.rows.into_iter().map(|row| row.file).collect()
            });
            self.pending_files(files);
        }
        if load {
            let dialog = rfd::FileDialog::new().add_filter(strings::LIST_FILE_TYPE, &["json"]);
            if let Some(path) = dialog.pick_file() {
                match RenameList::load(&path) {
                    Ok(list) => {
                        self.set_list(list);
                        shared.success(strings::MSG_LIST_LOADED);
                    }
                    Err(error) => shared.error(strings::msg_error(error)),
                }
            }
        }
        if save {
            let dialog = rfd::FileDialog::new()
                .add_filter(strings::LIST_FILE_TYPE, &["json"])
                .set_file_name(strings::LIST_DEFAULT_NAME);
            if let (Some(path), Some(list)) = (dialog.save_file(), &self.list) {
                match list.save(&path) {
                    Ok(()) => shared.success(strings::MSG_LIST_SAVED),
                    Err(error) => shared.error(strings::msg_error(error)),
                }
            }
        }
    }

    /// Conserva i file mentre si sceglie un'altra serie.
    fn pending_files(&mut self, files: Vec<PathBuf>) {
        self.kept_files = files;
        self.preview.clear();
        self.selected = None;
    }

    fn actions(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let Some(list) = &self.list else { return };
        let ready = self.ready_rows();
        let confirmed = list
            .rows
            .iter()
            .filter(|row| row.included && row.episode.is_some())
            .count();
        ui.horizontal(|ui| {
            ui.label(strings::list_summary(list.rows.len(), confirmed));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let enabled = !ready.is_empty() && !shared.busy;
                let label = strings::apply_list(ready.len());
                if style::primary_button(ui, enabled, icons::PLAY, &label).clicked() {
                    shared.requests.push(Request::ConfirmRename {
                        origin: Origin::Lists,
                        rows: ready,
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

    fn badge(&self, list: &RenameList, index: usize, duplicates: &[usize]) -> Badge {
        let row = &list.rows[index];
        if !row.included {
            return Badge::new(icons::MINUS_CIRCLE, strings::ST_EXCLUDED, None);
        }
        if row.episode.is_none() {
            return Badge::new(icons::HOURGLASS, strings::ST_UNASSIGNED, None);
        }
        if duplicates.contains(&index) {
            return Badge::new(icons::PROHIBIT, strings::ST_CONFLICT, Some(style::ERROR))
                .detail(strings::DETAIL_DUPLICATE_EPISODE);
        }
        match self
            .preview
            .get(index)
            .and_then(|p| p.as_ref())
            .map(|p| p.status)
        {
            Some(PreviewStatus::Ready) => {
                Badge::new(icons::CHECK_CIRCLE, strings::ST_READY, Some(style::OK))
            }
            Some(PreviewStatus::Unchanged) => {
                Badge::new(icons::CHECK_FAT, strings::ST_RENAMED, Some(style::OK))
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
            Some(PreviewStatus::InvalidTarget) | None => {
                Badge::new(icons::X_CIRCLE, strings::ST_ERROR, Some(style::ERROR))
                    .detail(strings::DETAIL_INVALID_TARGET)
            }
        }
    }

    fn table(&mut self, ui: &mut Ui) {
        let Some(mut list) = self.list.take() else {
            return;
        };
        if list.rows.is_empty() {
            ui.weak(strings::LIST_NO_FILES);
            self.list = Some(list);
            return;
        }
        let duplicates = list.duplicate_rows();
        let seasons = list.seasons();
        let spacing = ui.spacing().item_spacing.x;
        let fixed = 84.0 + 20.0 + 150.0 + 70.0 + 230.0 + 7.0 * spacing + 40.0;
        let flexible = (ui.available_width() - fixed).max(260.0);
        let (file_width, new_width) = (flexible * 0.42, flexible * 0.58);
        let mut action = None;
        let mut changed = false;

        egui::ScrollArea::both()
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("list_table")
                    .striped(true)
                    .min_row_height(style::ROW_HEIGHT)
                    .show(ui, |ui| {
                        ui.label("");
                        ui.label("");
                        style::header(ui, 150.0, strings::COL_STATUS);
                        style::header(ui, file_width, strings::COL_FILE);
                        style::header(ui, 70.0, strings::COL_SEASON);
                        style::header(ui, 230.0, strings::COL_EPISODE);
                        style::header(ui, new_width, strings::COL_NEW);
                        ui.end_row();

                        let last = list.rows.len() - 1;
                        for index in 0..list.rows.len() {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 2.0;
                                let up = egui::Button::new(icons::ARROW_UP).small();
                                if ui
                                    .add_enabled(index > 0, up)
                                    .on_hover_text(strings::MOVE_UP)
                                    .clicked()
                                {
                                    action = Some(RowAction::Up(index));
                                }
                                let down = egui::Button::new(icons::ARROW_DOWN).small();
                                if ui
                                    .add_enabled(index < last, down)
                                    .on_hover_text(strings::MOVE_DOWN)
                                    .clicked()
                                {
                                    action = Some(RowAction::Down(index));
                                }
                            });

                            let mut included = list.rows[index].included;
                            if ui
                                .add(egui::Checkbox::without_text(&mut included))
                                .changed()
                            {
                                list.set_included(index, included);
                                changed = true;
                            }
                            self.badge(&list, index, &duplicates).show(ui, 150.0);

                            let name = file_name(&list.rows[index].file);
                            let selected = self.selected == Some(index);
                            let response = ui
                                .allocate_ui(egui::vec2(file_width, style::ROW_HEIGHT), |ui| {
                                    ui.set_min_width(file_width);
                                    ui.add(
                                        egui::Button::selectable(selected, name.as_str())
                                            .truncate(),
                                    )
                                })
                                .inner
                                .on_hover_text(list.rows[index].file.display().to_string());
                            if response.clicked() {
                                action = Some(RowAction::Select(index));
                            }

                            // Stagione ed episodio: due menu popolati dall'elenco della serie.
                            let current = list.rows[index].episode;
                            let mut season = current.map(|e| e.season);
                            egui::ComboBox::from_id_salt(("row_season", index))
                                .width(54.0)
                                .selected_text(
                                    season
                                        .map_or(strings::LIST_NONE.to_string(), |s| s.to_string()),
                                )
                                .show_ui(ui, |ui| {
                                    ui.selectable_value(&mut season, None, strings::LIST_NONE);
                                    for s in &seasons {
                                        ui.selectable_value(&mut season, Some(*s), s.to_string());
                                    }
                                });
                            let mut episode = current.filter(|e| Some(e.season) == season);
                            if season != current.map(|e| e.season) {
                                // Cambiata la stagione: si parte dal suo primo episodio.
                                episode = season.and_then(|s| {
                                    let first = list.episodes.iter().find(|e| e.season == s)?;
                                    Some(EpisodeRef {
                                        season: s,
                                        number: first.number,
                                    })
                                });
                            }
                            let label = |reference: EpisodeRef| {
                                let title = list.episode(reference).map(|e| e.title.as_str());
                                format!("{} · {}", reference.number, title.unwrap_or_default())
                            };
                            egui::ComboBox::from_id_salt(("row_episode", index))
                                .width(214.0)
                                .selected_text(
                                    episode.map_or(strings::LIST_NONE.to_string(), label),
                                )
                                .show_ui(ui, |ui| {
                                    let of_season =
                                        list.episodes.iter().filter(|e| Some(e.season) == season);
                                    for e in of_season {
                                        let reference = EpisodeRef {
                                            season: e.season,
                                            number: e.number,
                                        };
                                        ui.selectable_value(
                                            &mut episode,
                                            Some(reference),
                                            label(reference),
                                        );
                                    }
                                });
                            if episode != current {
                                list.assign(index, episode);
                                changed = true;
                            }

                            let target =
                                self.preview.get(index).and_then(|p| p.as_ref()).map(|p| {
                                    let base =
                                        list.rows[index].file.parent().unwrap_or(&p.item.target);
                                    let relative = p.item.target.strip_prefix(base);
                                    relative.unwrap_or(&p.item.target).display().to_string()
                                });
                            style::cell(
                                ui,
                                new_width,
                                target.as_deref().unwrap_or(strings::LIST_NONE),
                            );
                            ui.end_row();
                        }
                    });
            });

        match action {
            Some(RowAction::Up(index)) => {
                list.move_row(index, index - 1);
                self.selected = Some(index - 1);
                changed = true;
            }
            Some(RowAction::Down(index)) => {
                list.move_row(index, index + 1);
                self.selected = Some(index + 1);
                changed = true;
            }
            Some(RowAction::Select(index)) => {
                self.selected = (self.selected != Some(index)).then_some(index);
            }
            None => {}
        }
        self.dirty |= changed;
        self.list = Some(list);
    }
}
