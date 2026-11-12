//! Episodi: ricerca di una serie e consultazione dell'elenco degli episodi.

use eframe::egui::{self, RichText, Ui};
use egui_phosphor::regular as icons;
use renbot_core::config::LANGUAGES;
use renbot_core::providers::{Episode, ProviderId, SearchResult};

use super::{title_with_year, Request, Shared};
use crate::backend::Origin;
use crate::strings;
use crate::style;

/// Barra di ricerca di una serie, condivisa con la sezione Liste.
#[derive(Default)]
pub struct SeriesSearch {
    pub query: String,
    pub searching: bool,
    pub results: Option<Result<Vec<SearchResult>, String>>,
}

impl SeriesSearch {
    /// Disegna database, campo di ricerca, lingua e pulsante; avvia la ricerca quando richiesto.
    pub fn bar(&mut self, ui: &mut Ui, shared: &mut Shared, origin: Origin) {
        for id in [ProviderId::Tmdb, ProviderId::Tvdb] {
            ui.selectable_value(&mut shared.settings.provider, id, strings::provider(id));
        }
        let field = egui::TextEdit::singleline(&mut self.query)
            .hint_text(strings::EP_HINT)
            .desired_width(260.0);
        let response = ui.add(field);
        let enter = response.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));

        ui.label(strings::LANGUAGE);
        egui::ComboBox::from_id_salt(("language", origin as u8))
            .selected_text(strings::language(&shared.settings.language).to_string())
            .show_ui(ui, |ui| {
                for code in LANGUAGES {
                    ui.selectable_value(
                        &mut shared.settings.language,
                        code.to_string(),
                        strings::language(code),
                    );
                }
            });

        let button = egui::Button::new(style::icon_text(icons::MAGNIFYING_GLASS, strings::SEARCH));
        let clicked = ui.add_enabled(!self.searching, button).clicked();
        if (clicked || enter) && !self.query.trim().is_empty() {
            self.searching = true;
            self.results = None;
            shared
                .backend
                .search_series(shared.settings, origin, self.query.trim().to_string());
        }
        if self.searching {
            ui.spinner();
        }
    }

    pub fn on_found(&mut self, result: Result<Vec<SearchResult>, String>) {
        self.searching = false;
        self.results = Some(result);
    }

    /// Elenco dei risultati; restituisce la serie su cui si è fatto clic.
    pub fn results(&self, ui: &mut Ui, selected: Option<&SearchResult>) -> Option<SearchResult> {
        let mut picked = None;
        match &self.results {
            None => {}
            Some(Err(error)) => {
                ui.colored_label(style::ERROR, error);
            }
            Some(Ok(found)) if found.is_empty() => {
                ui.weak(strings::EP_NO_RESULTS);
            }
            Some(Ok(found)) => {
                egui::ScrollArea::vertical()
                    .id_salt("series_results")
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        for series in found {
                            let label = format!(
                                "{}  ·  {}",
                                title_with_year(series),
                                strings::provider(series.provider)
                            );
                            let is_selected = selected == Some(series);
                            let button = egui::Button::selectable(is_selected, label).truncate();
                            let response =
                                ui.add_sized([ui.available_width(), style::ROW_HEIGHT], button);
                            if response.clicked() {
                                picked = Some(series.clone());
                            }
                        }
                    });
            }
        }
        picked
    }
}

#[derive(Default)]
pub struct EpisodesView {
    search: SeriesSearch,
    selected: Option<SearchResult>,
    loading: bool,
    episodes: Option<Result<Vec<Episode>, String>>,
    /// Stagione mostrata; `None` le mostra tutte.
    season: Option<u32>,
}

impl EpisodesView {
    pub fn on_found(&mut self, result: Result<Vec<SearchResult>, String>) {
        self.search.on_found(result);
    }

    pub fn on_loaded(&mut self, series: SearchResult, result: Result<Vec<Episode>, String>) {
        if self.selected.as_ref() == Some(&series) {
            self.loading = false;
            self.episodes = Some(result);
        }
    }

    fn load(&mut self, series: SearchResult, shared: &mut Shared) {
        self.loading = true;
        self.episodes = None;
        self.season = None;
        shared
            .backend
            .load_episodes(shared.settings, Origin::Episodes, series.clone());
        self.selected = Some(series);
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared) {
        egui::Panel::top("episodes_bar").show(ui, |ui| {
            ui.add_space(style::UNIT);
            ui.horizontal_wrapped(|ui| self.search.bar(ui, shared, Origin::Episodes));
            ui.add_space(style::UNIT);
        });

        if self.search.results.is_none() && self.selected.is_none() {
            egui::CentralPanel::default().show(ui, |ui| {
                style::empty_state(
                    ui,
                    icons::TELEVISION_SIMPLE,
                    strings::EP_EMPTY_TITLE,
                    strings::EP_EMPTY_HINT,
                );
            });
            return;
        }

        let mut picked = None;
        egui::Panel::left("episodes_results")
            .default_size(300.0)
            .min_size(220.0)
            .show(ui, |ui| {
                ui.add_space(style::UNIT);
                ui.label(RichText::new(strings::EP_RESULTS).strong());
                picked = self.search.results(ui, self.selected.as_ref());
            });
        if let Some(series) = picked {
            self.load(series, shared);
        }

        egui::CentralPanel::default().show(ui, |ui| self.episode_list(ui, shared));
    }

    fn episode_list(&mut self, ui: &mut Ui, shared: &mut Shared) {
        let Some(series) = self.selected.clone() else {
            ui.weak(strings::EP_PICK);
            return;
        };
        let episodes: &[Episode] = match &self.episodes {
            Some(Ok(episodes)) => episodes,
            _ => &[],
        };
        let mut seasons: Vec<u32> = episodes.iter().map(|e| e.season).collect();
        seasons.dedup();

        let mut refresh = false;
        ui.horizontal_wrapped(|ui| {
            ui.heading(title_with_year(&series));
            ui.weak(format!(
                "{} · {}",
                strings::provider(series.provider),
                strings::episode_count(episodes.len(), seasons.len())
            ));
        });
        ui.horizontal(|ui| {
            ui.label(strings::SEASON);
            let current = match self.season {
                Some(season) => season.to_string(),
                None => strings::ALL_SEASONS.to_string(),
            };
            egui::ComboBox::from_id_salt("episodes_season")
                .selected_text(current)
                .show_ui(ui, |ui| {
                    ui.selectable_value(&mut self.season, None, strings::ALL_SEASONS);
                    for season in &seasons {
                        ui.selectable_value(&mut self.season, Some(*season), season.to_string());
                    }
                });
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                let ready = !episodes.is_empty();
                if style::primary_button(ui, ready, icons::LIST_PLUS, strings::EP_NEW_LIST)
                    .clicked()
                {
                    shared.requests.push(Request::NewList {
                        series: series.clone(),
                        episodes: episodes.to_vec(),
                    });
                }
                let button = egui::Button::new(style::icon_text(
                    icons::ARROWS_CLOCKWISE,
                    strings::EP_REFRESH,
                ));
                refresh = ui
                    .add_enabled(!self.loading, button)
                    .on_hover_text(strings::EP_REFRESH_HINT)
                    .clicked();
            });
        });
        ui.separator();

        if self.loading {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(strings::LOADING);
            });
        }
        if let Some(Err(error)) = &self.episodes {
            ui.colored_label(style::ERROR, error);
        }

        let title_width = (ui.available_width() - 260.0).max(200.0);
        egui::ScrollArea::vertical()
            .id_salt("episode_table")
            .auto_shrink([false, false])
            .show(ui, |ui| {
                egui::Grid::new("episodes_table")
                    .striped(true)
                    .min_row_height(style::ROW_HEIGHT)
                    .show(ui, |ui| {
                        style::header(ui, 40.0, strings::COL_SEASON_SHORT);
                        style::header(ui, 40.0, strings::COL_NUMBER);
                        style::header(ui, title_width, strings::COL_TITLE);
                        style::header(ui, 100.0, strings::COL_DATE);
                        ui.end_row();
                        let shown = episodes
                            .iter()
                            .filter(|e| self.season.is_none_or(|s| s == e.season));
                        for episode in shown {
                            style::cell(ui, 40.0, &episode.season.to_string());
                            style::cell(ui, 40.0, &episode.number.to_string());
                            style::cell(ui, title_width, &episode.title);
                            let date = episode.air_date.as_deref().unwrap_or(strings::LIST_NONE);
                            style::cell(ui, 100.0, date);
                            ui.end_row();
                        }
                    });
            });

        if refresh {
            match shared.backend.clear_cache() {
                Ok(()) => shared.info(strings::MSG_CACHE_CLEARED),
                Err(error) => shared.error(strings::msg_error(error)),
            }
            self.load(series, shared);
        }
    }
}
