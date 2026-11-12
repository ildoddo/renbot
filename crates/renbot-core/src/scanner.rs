//! Ricerca ricorsiva dei file video tra i percorsi scelti dall'utente.

use std::path::{Path, PathBuf};

use walkdir::WalkDir;

/// Estensioni dei formati video riconosciuti.
pub const VIDEO_EXTENSIONS: [&str; 13] = [
    "mkv", "mp4", "avi", "mov", "m4v", "wmv", "mpg", "mpeg", "ts", "m2ts", "webm", "flv", "ogv",
];

/// Vero per un nome con estensione video; i file nascosti sono esclusi.
pub fn is_video(path: &Path) -> bool {
    let hidden = path
        .file_name()
        .is_some_and(|name| name.to_string_lossy().starts_with('.'));
    let video = path.extension().is_some_and(|ext| {
        let ext = ext.to_string_lossy().to_ascii_lowercase();
        VIDEO_EXTENSIONS.contains(&ext.as_str())
    });
    video && !hidden
}

/// File video contenuti nei percorsi indicati: i file sono presi così come sono,
/// le cartelle sono esplorate in profondità. Il risultato è ordinato e senza doppioni.
pub fn scan(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut found: Vec<PathBuf> = paths
        .iter()
        .flat_map(|path| WalkDir::new(path).into_iter().filter_map(Result::ok))
        .filter(|entry| entry.file_type().is_file() && is_video(entry.path()))
        .map(|entry| entry.into_path())
        .collect();
    found.sort();
    found.dedup();
    found
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn trova_i_video_in_profondità() {
        let dir = TempDir::new("scan");
        let root = dir.path();
        let season = root.join("Serie").join("Stagione 1");
        fs::create_dir_all(&season).unwrap();
        for file in [
            root.join("film.MKV"),
            root.join("note.txt"),
            root.join(".nascosto.mkv"),
            season.join("b.mp4"),
            season.join("a.avi"),
            season.join("copertina.jpg"),
        ] {
            fs::write(file, "").unwrap();
        }

        let found = scan(&[root.to_path_buf(), root.join("film.MKV")]);
        assert_eq!(
            found,
            [
                season.join("a.avi"),
                season.join("b.mp4"),
                root.join("film.MKV")
            ]
        );
        assert!(scan(&[root.join("inesistente")]).is_empty());
    }
}
