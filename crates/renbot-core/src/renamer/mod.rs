//! Rinomina dei file: anteprima, esecuzione senza sovrascritture e annullamento.

pub mod format;
pub mod list;

use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Un file da spostare da `source` a `target`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameItem {
    pub source: PathBuf,
    pub target: PathBuf,
}

impl RenameItem {
    /// Destinazione ottenuta dal percorso relativo prodotto dal formato.
    /// Senza cartella di base il nuovo percorso parte dalla cartella del file.
    pub fn new(source: impl Into<PathBuf>, base: Option<&Path>, relative: &Path) -> Self {
        let source = source.into();
        let base = base.or(source.parent()).unwrap_or(Path::new(""));
        let target = base.join(relative);
        Self { source, target }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreviewStatus {
    /// Può essere rinominato.
    Ready,
    /// Ha già il nome previsto.
    Unchanged,
    /// Nella destinazione esiste già un altro file.
    TargetExists,
    /// Più file dell'elenco hanno la stessa destinazione.
    DuplicateTarget,
    /// Il file di origine non esiste più.
    SourceMissing,
    /// Il formato non ha prodotto un nome di file valido.
    InvalidTarget,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewRow {
    pub item: RenameItem,
    pub status: PreviewStatus,
}

/// Calcola lo stato di ogni rinomina senza modificare nulla sul disco.
pub fn preview(items: Vec<RenameItem>) -> Vec<PreviewRow> {
    let mut targets: HashMap<String, usize> = HashMap::new();
    for item in &items {
        *targets.entry(target_key(&item.target)).or_default() += 1;
    }

    items
        .into_iter()
        .map(|item| {
            let status = if item.target.file_name().is_none() || item.target.is_dir() {
                PreviewStatus::InvalidTarget
            } else if !item.source.is_file() {
                PreviewStatus::SourceMissing
            } else if item.source == item.target {
                PreviewStatus::Unchanged
            } else if targets[&target_key(&item.target)] > 1 {
                PreviewStatus::DuplicateTarget
            } else if occupied(&item.source, &item.target) {
                PreviewStatus::TargetExists
            } else {
                PreviewStatus::Ready
            };
            PreviewRow { item, status }
        })
        .collect()
}

/// Confronto senza distinzione tra maiuscole e minuscole, come sui file system
/// di Windows e macOS: due destinazioni così vicine vanno trattate come uguali.
fn target_key(target: &Path) -> String {
    target.to_string_lossy().to_lowercase()
}

/// Vero se la destinazione esiste ed è un file diverso dall'origine. Sui file system
/// che ignorano le maiuscole un cambio di sole maiuscole punta allo stesso file.
fn occupied(source: &Path, target: &Path) -> bool {
    if !target.exists() {
        return false;
    }
    match (fs::canonicalize(source), fs::canonicalize(target)) {
        (Ok(a), Ok(b)) => a != b,
        _ => true,
    }
}

#[derive(Debug, Default)]
pub struct ApplyReport {
    pub renamed: Vec<RenameItem>,
    pub failed: Vec<(RenameItem, io::Error)>,
}

/// Registro dell'ultima operazione, usato per annullarla.
#[derive(Serialize, Deserialize)]
struct UndoLog {
    version: u32,
    entries: Vec<RenameItem>,
    /// Cartelle create dall'operazione, da rimuovere se restano vuote.
    created_dirs: Vec<PathBuf>,
}

/// Percorso predefinito del registro di annullamento.
pub fn default_undo_log() -> Option<PathBuf> {
    let dirs = ProjectDirs::from("", "", "renbot")?;
    Some(dirs.data_dir().join("undo.json"))
}

/// Rinomina le righe con stato [`PreviewStatus::Ready`] e salva il registro di annullamento.
/// Non sovrascrive mai un file esistente. Restituisce errore solo se il registro
/// non è scrivibile: in quel caso, se il problema emerge prima di iniziare, nessun file viene toccato.
pub fn apply(rows: &[PreviewRow], undo_log: &Path) -> io::Result<ApplyReport> {
    if let Some(parent) = undo_log.parent() {
        fs::create_dir_all(parent)?;
    }

    let mut report = ApplyReport::default();
    let mut created_dirs = Vec::new();
    for row in rows.iter().filter(|r| r.status == PreviewStatus::Ready) {
        let item = &row.item;
        match rename_one(item, &mut created_dirs) {
            Ok(()) => report.renamed.push(item.clone()),
            Err(error) => {
                tracing::warn!(source = %item.source.display(), %error, "rinomina non riuscita");
                report.failed.push((item.clone(), error));
            }
        }
    }

    if !report.renamed.is_empty() {
        write_log(
            undo_log,
            &UndoLog {
                version: 1,
                entries: report.renamed.clone(),
                created_dirs,
            },
        )?;
    }
    Ok(report)
}

fn rename_one(item: &RenameItem, created_dirs: &mut Vec<PathBuf>) -> io::Result<()> {
    if occupied(&item.source, &item.target) {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    if let Some(parent) = item.target.parent() {
        let missing: Vec<PathBuf> = parent
            .ancestors()
            .take_while(|dir| !dir.as_os_str().is_empty() && !dir.exists())
            .map(Path::to_path_buf)
            .collect();
        fs::create_dir_all(parent)?;
        created_dirs.extend(missing);
    }
    move_file(&item.source, &item.target)
}

fn move_file(source: &Path, target: &Path) -> io::Result<()> {
    move_with(source, target, |from, to| fs::rename(from, to))
}

/// Sposta un file; tra dischi diversi, dove la rinomina non è possibile, copia e poi cancella.
fn move_with(
    source: &Path,
    target: &Path,
    rename: impl Fn(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    match rename(source, target) {
        Err(error) if error.kind() == io::ErrorKind::CrossesDevices => {
            if let Err(error) = fs::copy(source, target) {
                let _ = fs::remove_file(target);
                return Err(error);
            }
            fs::remove_file(source)
        }
        result => result,
    }
}

fn write_log(path: &Path, log: &UndoLog) -> io::Result<()> {
    fs::write(path, serde_json::to_string_pretty(log)?)
}

/// Vero se esiste un'operazione annullabile.
pub fn can_undo(undo_log: &Path) -> bool {
    undo_log.is_file()
}

#[derive(Debug, Default)]
pub struct UndoReport {
    pub restored: Vec<RenameItem>,
    pub failed: Vec<(RenameItem, io::Error)>,
}

/// Annulla l'ultima operazione riportando i file ai nomi originali e rimuovendo
/// le cartelle create, se vuote. Il registro viene eliminato quando tutto è ripristinato;
/// altrimenti conserva le sole voci non ripristinate.
pub fn undo(undo_log: &Path) -> io::Result<UndoReport> {
    let log: UndoLog = serde_json::from_str(&fs::read_to_string(undo_log)?)?;

    let mut report = UndoReport::default();
    for item in log.entries.into_iter().rev() {
        match restore_one(&item) {
            Ok(()) => report.restored.push(item),
            Err(error) => {
                tracing::warn!(target_path = %item.target.display(), %error, "ripristino non riuscito");
                report.failed.push((item, error));
            }
        }
    }

    // Dalla più profonda alla più esterna; le cartelle non vuote restano dove sono.
    let mut dirs = log.created_dirs;
    dirs.sort_by_key(|dir| std::cmp::Reverse(dir.components().count()));
    for dir in &dirs {
        let _ = fs::remove_dir(dir);
    }

    if report.failed.is_empty() {
        fs::remove_file(undo_log)?;
    } else {
        let entries = report.failed.iter().map(|(item, _)| item.clone()).collect();
        write_log(
            undo_log,
            &UndoLog {
                version: log.version,
                entries,
                created_dirs: dirs,
            },
        )?;
    }
    Ok(report)
}

fn restore_one(item: &RenameItem) -> io::Result<()> {
    if occupied(&item.target, &item.source) {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    if let Some(parent) = item.source.parent() {
        fs::create_dir_all(parent)?;
    }
    move_file(&item.target, &item.source)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    fn touch(path: &Path, content: &str) {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(path, content).unwrap();
    }

    fn item(dir: &TempDir, source: &str, target: &[&str]) -> RenameItem {
        let relative: PathBuf = target.iter().collect();
        RenameItem::new(dir.path().join(source), None, &relative)
    }

    fn statuses(rows: &[PreviewRow]) -> Vec<PreviewStatus> {
        rows.iter().map(|r| r.status).collect()
    }

    #[test]
    fn destinazione_relativa_alla_cartella_del_file_o_alla_base() {
        let source = Path::new("video").join("a.mkv");
        let relative = Path::new("Serie").join("b.mkv");
        assert_eq!(
            RenameItem::new(&source, None, &relative).target,
            Path::new("video").join("Serie").join("b.mkv")
        );
        assert_eq!(
            RenameItem::new(&source, Some(Path::new("libreria")), &relative).target,
            Path::new("libreria").join("Serie").join("b.mkv")
        );
    }

    #[test]
    fn l_anteprima_riconosce_ogni_stato_e_non_tocca_il_disco() {
        let dir = TempDir::new("preview");
        for name in ["a.mkv", "b.mkv", "c.mkv", "d.mkv", "e.mkv", "occupato.mkv"] {
            touch(&dir.path().join(name), name);
        }
        let rows = preview(vec![
            item(&dir, "a.mkv", &["Serie", "nuovo.mkv"]),
            item(&dir, "b.mkv", &["b.mkv"]),
            item(&dir, "c.mkv", &["occupato.mkv"]),
            item(&dir, "d.mkv", &["doppio.mkv"]),
            item(&dir, "e.mkv", &["DOPPIO.mkv"]),
            item(&dir, "assente.mkv", &["x.mkv"]),
            item(&dir, "a.mkv", &[]),
        ]);
        assert_eq!(
            statuses(&rows),
            [
                PreviewStatus::Ready,
                PreviewStatus::Unchanged,
                PreviewStatus::TargetExists,
                PreviewStatus::DuplicateTarget,
                PreviewStatus::DuplicateTarget,
                PreviewStatus::SourceMissing,
                PreviewStatus::InvalidTarget,
            ]
        );
        assert!(dir.path().join("a.mkv").is_file());
        assert!(!dir.path().join("Serie").exists());
    }

    #[test]
    fn rinomina_solo_le_righe_pronte_senza_sovrascrivere() {
        let dir = TempDir::new("apply");
        touch(&dir.path().join("a.mkv"), "A");
        touch(&dir.path().join("c.mkv"), "C");
        touch(&dir.path().join("occupato.mkv"), "ORIGINALE");
        let log = dir.path().join("undo.json");

        let rows = preview(vec![
            item(&dir, "a.mkv", &["Serie", "Season 1", "nuovo.mkv"]),
            item(&dir, "c.mkv", &["occupato.mkv"]),
        ]);
        let report = apply(&rows, &log).unwrap();

        assert_eq!(report.renamed.len(), 1);
        assert!(report.failed.is_empty());
        let moved = dir.path().join("Serie").join("Season 1").join("nuovo.mkv");
        assert_eq!(fs::read_to_string(moved).unwrap(), "A");
        assert!(dir.path().join("c.mkv").is_file());
        assert_eq!(
            fs::read_to_string(dir.path().join("occupato.mkv")).unwrap(),
            "ORIGINALE"
        );
        assert!(can_undo(&log));
    }

    #[test]
    fn un_file_comparso_dopo_l_anteprima_non_viene_sovrascritto() {
        let dir = TempDir::new("race");
        touch(&dir.path().join("a.mkv"), "A");
        let log = dir.path().join("undo.json");
        let rows = preview(vec![item(&dir, "a.mkv", &["nuovo.mkv"])]);
        assert_eq!(statuses(&rows), [PreviewStatus::Ready]);

        touch(&dir.path().join("nuovo.mkv"), "ARRIVATO DOPO");
        let report = apply(&rows, &log).unwrap();

        assert!(report.renamed.is_empty());
        assert_eq!(report.failed[0].1.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(
            fs::read_to_string(dir.path().join("nuovo.mkv")).unwrap(),
            "ARRIVATO DOPO"
        );
        assert!(dir.path().join("a.mkv").is_file());
        assert!(!can_undo(&log));
    }

    #[test]
    fn annulla_ripristina_i_nomi_e_rimuove_le_cartelle_create() {
        let dir = TempDir::new("undo");
        touch(&dir.path().join("a.mkv"), "A");
        touch(&dir.path().join("b.mkv"), "B");
        let log = dir.path().join("undo.json");
        let rows = preview(vec![
            item(&dir, "a.mkv", &["Serie", "Season 1", "uno.mkv"]),
            item(&dir, "b.mkv", &["Serie", "Season 1", "due.mkv"]),
        ]);
        apply(&rows, &log).unwrap();
        assert!(!dir.path().join("a.mkv").exists());

        let report = undo(&log).unwrap();

        assert_eq!(report.restored.len(), 2);
        assert!(report.failed.is_empty());
        assert_eq!(fs::read_to_string(dir.path().join("a.mkv")).unwrap(), "A");
        assert_eq!(fs::read_to_string(dir.path().join("b.mkv")).unwrap(), "B");
        assert!(!dir.path().join("Serie").exists());
        assert!(!can_undo(&log));
    }

    #[test]
    fn annulla_conserva_le_voci_non_ripristinabili() {
        let dir = TempDir::new("undo-partial");
        touch(&dir.path().join("a.mkv"), "A");
        touch(&dir.path().join("b.mkv"), "B");
        let log = dir.path().join("undo.json");
        let rows = preview(vec![
            item(&dir, "a.mkv", &["Serie", "uno.mkv"]),
            item(&dir, "b.mkv", &["Serie", "due.mkv"]),
        ]);
        apply(&rows, &log).unwrap();
        touch(&dir.path().join("a.mkv"), "NUOVO FILE");

        let report = undo(&log).unwrap();

        assert_eq!(report.restored.len(), 1);
        assert_eq!(report.failed.len(), 1);
        assert_eq!(
            fs::read_to_string(dir.path().join("a.mkv")).unwrap(),
            "NUOVO FILE"
        );
        assert!(dir.path().join("Serie").join("uno.mkv").is_file());
        assert!(can_undo(&log));
    }

    #[test]
    fn tra_dischi_diversi_copia_e_cancella() {
        let dir = TempDir::new("cross-device");
        let source = dir.path().join("a.mkv");
        let target = dir.path().join("b.mkv");
        touch(&source, "CONTENUTO");

        let cross_device = |_: &Path, _: &Path| Err(io::Error::from(io::ErrorKind::CrossesDevices));
        move_with(&source, &target, cross_device).unwrap();

        assert!(!source.exists());
        assert_eq!(fs::read_to_string(&target).unwrap(), "CONTENUTO");
    }

    #[test]
    fn gli_altri_errori_di_rinomina_non_attivano_la_copia() {
        let dir = TempDir::new("rename-error");
        let source = dir.path().join("a.mkv");
        let target = dir.path().join("b.mkv");
        touch(&source, "CONTENUTO");

        let denied = |_: &Path, _: &Path| Err(io::Error::from(io::ErrorKind::PermissionDenied));
        let error = move_with(&source, &target, denied).unwrap_err();

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert!(source.is_file());
        assert!(!target.exists());
    }
}
