//! Impostazioni: chiavi API, formato dei nomi, ricerca e aspetto.

use std::collections::HashMap;

use eframe::egui::{self, RichText, Ui};
use egui_phosphor::regular as icons;
use renbot_core::config::keys::{ApiKey, KeySource};
use renbot_core::config::{Settings, Theme, LANGUAGES};
use renbot_core::providers::ProviderId;
use renbot_core::renamer::format::{
    render, unknown_tokens, FormatValues, DEFAULT_MOVIE_FORMAT, DEFAULT_SERIES_FORMAT, PRESETS,
};

use super::Shared;
use crate::strings;
use crate::style;

enum Verify {
    Idle,
    Running,
    Valid,
    Failed(String),
}

struct KeyField {
    /// Testo del campo; la chiave salvata vi è caricata all'avvio, mascherata.
    input: String,
    visible: bool,
    source: Option<KeySource>,
    verify: Verify,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum FormatField {
    Movie,
    Series,
}

pub struct SettingsView {
    keys: HashMap<ApiKey, KeyField>,
    /// Campo del formato in cui inserire il token su cui si fa clic.
    focused: FormatField,
}

impl SettingsView {
    pub fn new(shared: &Shared) -> Self {
        let keys = ApiKey::ALL
            .into_iter()
            .map(|key| (key, Self::load(key, shared)))
            .collect();
        Self {
            keys,
            focused: FormatField::Series,
        }
    }

    /// Vista vuota usata solo durante la costruzione dell'applicazione.
    pub fn placeholder() -> Self {
        Self {
            keys: HashMap::new(),
            focused: FormatField::Series,
        }
    }

    fn load(key: ApiKey, shared: &Shared) -> KeyField {
        let stored = shared.backend.keys().get(key);
        let source = stored.as_ref().map(|(_, source)| *source);
        // Una chiave presa dall'ambiente non viene copiata nel campo.
        let input = stored
            .filter(|(_, source)| *source != KeySource::Environment)
            .map(|(value, _)| value)
            .unwrap_or_default();
        KeyField {
            input,
            visible: false,
            source,
            verify: Verify::Idle,
        }
    }

    pub fn on_verified(&mut self, key: ApiKey, result: Result<(), String>) {
        if let Some(field) = self.keys.get_mut(&key) {
            field.verify = match result {
                Ok(()) => Verify::Valid,
                Err(error) => Verify::Failed(error),
            };
        }
    }

    pub fn ui(&mut self, ui: &mut Ui, shared: &mut Shared) {
        egui::CentralPanel::default().show(ui, |ui| {
            egui::ScrollArea::vertical()
                .auto_shrink([false, false])
                .show(ui, |ui| {
                    ui.set_max_width(760.0);
                    section(ui, icons::KEY, strings::SET_KEYS);
                    ui.weak(strings::SET_KEYS_HINT);
                    for key in ApiKey::ALL {
                        self.key_card(ui, key, shared);
                    }

                    section(ui, icons::TEXT_AA, strings::SET_FORMATS);
                    self.formats(ui, shared.settings);

                    section(ui, icons::MAGNIFYING_GLASS, strings::SET_SEARCH);
                    search(ui, shared.settings);

                    section(ui, icons::PALETTE, strings::SET_APPEARANCE);
                    ui.horizontal(|ui| {
                        ui.label(strings::SET_THEME);
                        for theme in [Theme::System, Theme::Light, Theme::Dark] {
                            ui.radio_value(
                                &mut shared.settings.theme,
                                theme,
                                strings::theme(theme),
                            );
                        }
                    });
                    if let Some(path) = renbot_core::config::log_path() {
                        section(ui, icons::NOTE, strings::SET_LOG);
                        ui.weak(strings::SET_LOG_HINT);
                        ui.add(egui::Label::new(path.display().to_string()).selectable(true));
                    }
                    ui.add_space(6.0 * style::UNIT);
                });
        });
    }

    fn key_card(&mut self, ui: &mut Ui, key: ApiKey, shared: &mut Shared) {
        let Some(field) = self.keys.get_mut(&key) else {
            return;
        };
        let mut reload = false;
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                ui.label(RichText::new(strings::key_name(key)).strong());
                if key == ApiKey::Omdb {
                    ui.weak(format!("({})", strings::SET_OPTIONAL));
                }
            });
            ui.weak(strings::key_help(key));

            ui.horizontal(|ui| {
                let edit = egui::TextEdit::singleline(&mut field.input)
                    .password(!field.visible)
                    .hint_text(strings::SET_KEY_HINT)
                    .desired_width((ui.available_width() - 130.0).max(160.0));
                if ui.add(edit).changed() {
                    field.verify = Verify::Idle;
                }
                let (icon, text) = if field.visible {
                    (icons::EYE_SLASH, strings::SET_HIDE)
                } else {
                    (icons::EYE, strings::SET_SHOW)
                };
                if ui.button(style::icon_text(icon, text)).clicked() {
                    field.visible = !field.visible;
                }
            });

            ui.horizontal_wrapped(|ui| {
                let filled = !field.input.trim().is_empty();
                let save =
                    egui::Button::new(style::icon_text(icons::FLOPPY_DISK, strings::SET_SAVE));
                if ui.add_enabled(filled, save).clicked() {
                    match shared.backend.keys().set(key, &field.input) {
                        Ok(_) => {
                            shared.success(strings::MSG_KEY_SAVED);
                            reload = true;
                        }
                        Err(error) => shared.error(strings::msg_error(error)),
                    }
                }
                let running = matches!(field.verify, Verify::Running);
                let verify =
                    egui::Button::new(style::icon_text(icons::SEAL_CHECK, strings::SET_VERIFY));
                if ui.add_enabled(filled && !running, verify).clicked() {
                    field.verify = Verify::Running;
                    shared.backend.verify_key(key, field.input.clone());
                }
                let saved = matches!(field.source, Some(KeySource::Keychain | KeySource::File));
                let delete = egui::Button::new(style::icon_text(icons::TRASH, strings::SET_DELETE));
                if ui.add_enabled(saved || filled, delete).clicked() {
                    match shared.backend.keys().delete(key) {
                        Ok(()) => {
                            shared.success(strings::MSG_KEY_DELETED);
                            reload = true;
                        }
                        Err(error) => shared.error(strings::msg_error(error)),
                    }
                }

                match &field.verify {
                    Verify::Idle => {}
                    Verify::Running => {
                        ui.spinner();
                        ui.label(strings::SET_VERIFYING);
                    }
                    Verify::Valid => {
                        let text = style::icon_text(icons::CHECK_CIRCLE, strings::SET_KEY_OK);
                        ui.colored_label(style::OK, text);
                    }
                    Verify::Failed(error) => {
                        ui.colored_label(style::ERROR, style::icon_text(icons::X_CIRCLE, error));
                    }
                }
            });

            let source = match field.source {
                Some(source) => strings::key_source(source, key),
                None => strings::SET_NO_KEY.to_string(),
            };
            ui.weak(source);
        });

        if reload {
            let verify = std::mem::replace(&mut field.verify, Verify::Idle);
            let mut fresh = Self::load(key, shared);
            fresh.visible = field.visible;
            // L'esito della verifica resta valido solo se la chiave nel campo non è cambiata.
            if fresh.input == field.input {
                fresh.verify = verify;
            }
            self.keys.insert(key, fresh);
        }
    }

    fn formats(&mut self, ui: &mut Ui, settings: &mut Settings) {
        ui.weak(strings::SET_FORMAT_HINT);
        ui.horizontal(|ui| {
            ui.label(strings::SET_PRESET);
            egui::ComboBox::from_id_salt("format_preset")
                .selected_text(strings::SET_PRESET_PICK)
                .show_ui(ui, |ui| {
                    for (preset, name) in PRESETS.iter().zip(strings::PRESETS) {
                        if ui.selectable_label(false, name).clicked() {
                            settings.movie_format = preset.movie.to_string();
                            settings.series_format = preset.series.to_string();
                        }
                    }
                });
            let reset = style::icon_text(icons::ARROW_COUNTER_CLOCKWISE, strings::SET_RESET_FORMAT);
            if ui.button(reset).clicked() {
                settings.movie_format = DEFAULT_MOVIE_FORMAT.to_string();
                settings.series_format = DEFAULT_SERIES_FORMAT.to_string();
            }
        });

        let sample = |season| FormatValues {
            name: if season {
                strings::SAMPLE_SERIES
            } else {
                strings::SAMPLE_MOVIE
            }
            .to_string(),
            year: Some(2019),
            season: season.then_some(1),
            episode: season.then_some(2),
            title: if season { strings::SAMPLE_EPISODE } else { "" }.to_string(),
            quality: Some("1080p WEB-DL".to_string()),
            group: Some("GRP".to_string()),
            extension: "mkv".to_string(),
        };
        let fields = [
            (
                FormatField::Movie,
                strings::SET_MOVIE_FORMAT,
                &mut settings.movie_format,
                sample(false),
            ),
            (
                FormatField::Series,
                strings::SET_SERIES_FORMAT,
                &mut settings.series_format,
                sample(true),
            ),
        ];
        egui::Grid::new("formats").num_columns(2).show(ui, |ui| {
            for (which, name, format, values) in fields {
                ui.label(name);
                let edit = egui::TextEdit::singleline(format)
                    .font(egui::TextStyle::Monospace)
                    .desired_width(560.0);
                if ui.add(edit).has_focus() {
                    self.focused = which;
                }
                ui.end_row();

                ui.weak(strings::SET_PREVIEW);
                ui.vertical(|ui| {
                    ui.label(render(format, &values).display().to_string());
                    let unknown = unknown_tokens(format);
                    if !unknown.is_empty() {
                        let list = unknown.iter().map(|t| format!("{{{t}}}"));
                        let text = format!(
                            "{} {}",
                            strings::SET_UNKNOWN_TOKENS,
                            list.collect::<Vec<_>>().join(" ")
                        );
                        ui.colored_label(style::WARNING, style::icon_text(icons::WARNING, &text));
                    }
                });
                ui.end_row();
            }
        });

        ui.horizontal_wrapped(|ui| {
            for (token, description) in strings::TOKENS {
                let chip = egui::Button::new(RichText::new(token).monospace()).small();
                if ui.add(chip).on_hover_text(description).clicked() {
                    match self.focused {
                        FormatField::Movie => settings.movie_format.push_str(token),
                        FormatField::Series => settings.series_format.push_str(token),
                    }
                }
            }
        });
    }
}

fn search(ui: &mut Ui, settings: &mut Settings) {
    ui.horizontal_wrapped(|ui| {
        ui.label(strings::DATABASE);
        for id in [ProviderId::Tmdb, ProviderId::Tvdb] {
            ui.selectable_value(&mut settings.provider, id, strings::provider(id));
        }
        ui.checkbox(&mut settings.use_fallback, strings::USE_FALLBACK);
    });
    ui.horizontal(|ui| {
        ui.label(strings::LANGUAGE);
        egui::ComboBox::from_id_salt("settings_language")
            .selected_text(strings::language(&settings.language).to_string())
            .show_ui(ui, |ui| {
                for code in LANGUAGES {
                    ui.selectable_value(
                        &mut settings.language,
                        code.to_string(),
                        strings::language(code),
                    );
                }
            });
    });
}

fn section(ui: &mut Ui, icon: &str, title: &str) {
    ui.add_space(4.0 * style::UNIT);
    ui.heading(style::icon_text(icon, title));
    ui.add_space(style::UNIT);
}
