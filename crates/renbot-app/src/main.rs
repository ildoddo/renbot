//! renbot: rinomina film, serie TV e anime con i dati dei database online.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod backend;
mod strings;
mod style;
mod views;

use std::fs::{self, File};
use std::sync::Mutex;

use eframe::egui;
use renbot_core::config::log_path;

/// Scrive il registro dell'esecuzione corrente su file, sostituendo quello precedente.
/// Se il file non è scrivibile il registro va sullo standard error.
fn init_logging() {
    let file = log_path().and_then(|path| {
        fs::create_dir_all(path.parent()?).ok()?;
        File::create(path).ok()
    });
    let builder = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_ansi(false);
    match file {
        Some(file) => builder.with_writer(Mutex::new(file)).init(),
        None => builder.with_writer(std::io::stderr).init(),
    }
}

fn main() -> anyhow::Result<()> {
    init_logging();
    tracing::info!(version = env!("CARGO_PKG_VERSION"), "avvio di renbot");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(strings::APP_NAME)
            .with_inner_size([1180.0, 720.0])
            .with_min_inner_size([900.0, 560.0]),
        ..Default::default()
    };
    eframe::run_native(
        strings::APP_NAME,
        options,
        Box::new(|cc| Ok(Box::new(app::RenbotApp::new(cc)?))),
    )
    .map_err(|error| anyhow::anyhow!("{error}"))
}
