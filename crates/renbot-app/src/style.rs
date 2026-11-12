//! Aspetto comune: spaziature, colori e piccoli componenti riutilizzati dalle viste.

use eframe::egui::{self, Align, Color32, Layout, RichText, Ui};
use renbot_core::config::Theme;

pub const ACCENT: Color32 = Color32::from_rgb(0x2F, 0x6F, 0xED);
pub const OK: Color32 = Color32::from_rgb(0x2E, 0x9E, 0x5B);
pub const WARNING: Color32 = Color32::from_rgb(0xD9, 0x8E, 0x04);
pub const ERROR: Color32 = Color32::from_rgb(0xD6, 0x45, 0x45);

/// Unità della griglia di spaziatura.
pub const UNIT: f32 = 4.0;
pub const ROW_HEIGHT: f32 = 24.0;

pub fn install(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    egui_phosphor::add_to_fonts(&mut fonts, egui_phosphor::Variant::Regular);
    ctx.set_fonts(fonts);
    ctx.all_styles_mut(|style| {
        style.spacing.item_spacing = egui::vec2(2.0 * UNIT, 2.0 * UNIT);
        style.spacing.button_padding = egui::vec2(3.0 * UNIT, 1.5 * UNIT);
        style.spacing.interact_size.y = ROW_HEIGHT;
    });
}

pub fn apply_theme(ctx: &egui::Context, theme: Theme) {
    ctx.set_theme(match theme {
        Theme::System => egui::ThemePreference::System,
        Theme::Light => egui::ThemePreference::Light,
        Theme::Dark => egui::ThemePreference::Dark,
    });
}

/// Testo di un pulsante con icona.
pub fn icon_text(icon: &str, text: &str) -> String {
    format!("{icon}  {text}")
}

/// Pulsante dell'azione principale di una vista.
pub fn primary_button(ui: &mut Ui, enabled: bool, icon: &str, text: &str) -> egui::Response {
    let label = RichText::new(icon_text(icon, text))
        .color(Color32::WHITE)
        .strong();
    ui.add_enabled(enabled, egui::Button::new(label).fill(ACCENT))
}

/// Stato di una riga: icona, etichetta e colore, con dettaglio al passaggio del mouse.
pub struct Badge {
    pub icon: &'static str,
    pub label: &'static str,
    pub color: Option<Color32>,
    pub detail: Option<String>,
}

impl Badge {
    pub fn new(icon: &'static str, label: &'static str, color: Option<Color32>) -> Self {
        Self {
            icon,
            label,
            color,
            detail: None,
        }
    }

    pub fn detail(mut self, detail: impl Into<String>) -> Self {
        self.detail = Some(detail.into());
        self
    }

    pub fn show(&self, ui: &mut Ui, width: f32) {
        let color = self.color.unwrap_or_else(|| ui.visuals().weak_text_color());
        let text = RichText::new(icon_text(self.icon, self.label)).color(color);
        let response = cell_rich(ui, width, text);
        if let Some(detail) = &self.detail {
            response.on_hover_text(detail);
        }
    }
}

/// Cella di tabella a larghezza fissa: il testo lungo è troncato e mostrato per intero al passaggio.
pub fn cell(ui: &mut Ui, width: f32, text: &str) -> egui::Response {
    cell_rich(ui, width, RichText::new(text)).on_hover_text(text)
}

pub fn cell_rich(ui: &mut Ui, width: f32, text: RichText) -> egui::Response {
    ui.allocate_ui_with_layout(
        egui::vec2(width, ROW_HEIGHT),
        Layout::left_to_right(Align::Center),
        |ui| {
            ui.set_min_width(width);
            ui.add(egui::Label::new(text).truncate())
        },
    )
    .inner
}

/// Intestazione di colonna.
pub fn header(ui: &mut Ui, width: f32, text: &str) {
    cell_rich(ui, width, RichText::new(text).strong());
}

/// Messaggio centrato per le viste senza contenuto.
pub fn empty_state(ui: &mut Ui, icon: &str, title: &str, hint: &str) {
    ui.vertical_centered(|ui| {
        ui.add_space(ui.available_height() * 0.28);
        ui.label(
            RichText::new(icon)
                .size(56.0)
                .color(ui.visuals().weak_text_color()),
        );
        ui.add_space(2.0 * UNIT);
        ui.label(RichText::new(title).size(18.0).strong());
        ui.label(RichText::new(hint).color(ui.visuals().weak_text_color()));
    });
}

/// Riquadro informativo colorato, usato per avvisi e inviti all'azione.
pub fn banner(ui: &mut Ui, color: Color32, add_contents: impl FnOnce(&mut Ui)) {
    egui::Frame::group(ui.style())
        .stroke(egui::Stroke::new(1.0, color))
        .fill(color.gamma_multiply(0.12))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal_wrapped(add_contents);
        });
}
