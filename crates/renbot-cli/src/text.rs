//! Testi mostrati dalla riga di comando, in italiano.

use renbot_core::config::keys::{ApiKey, KeySource};
use renbot_core::providers::ProviderId;

pub const ARROW: &str = "→";
pub const READY: &str = "Pronto";
pub const REVIEW: &str = "Da verificare";
pub const UNCHANGED: &str = "Invariato";
pub const CONFLICT: &str = "Conflitto";
pub const NO_MATCH: &str = "Nessun risultato";
pub const NO_EPISODE: &str = "Episodio non trovato";
pub const ERROR: &str = "Errore";
pub const RENAMED: &str = "Rinominato";
pub const RESTORED: &str = "Ripristinato";
pub const EXCLUDED: &str = "Escluso";
pub const UNASSIGNED: &str = "Senza episodio";

pub const TARGET_EXISTS: &str = "esiste già un file con il nuovo nome";
pub const DUPLICATE_TARGET: &str = "più file avrebbero lo stesso nuovo nome";
pub const DUPLICATE_EPISODE: &str = "lo stesso episodio è assegnato a più righe";
pub const SOURCE_MISSING: &str = "il file originale non esiste più";
pub const INVALID_TARGET: &str = "il formato non produce un nome di file valido";

pub const NO_VIDEO: &str = "Nessun file video trovato.";
pub const PREVIEW_ONLY: &str =
    "Anteprima: nessun file è stato modificato. Aggiungi --apply per rinominare.";
pub const REVIEW_HINT: &str =
    "Le righe \"Da verificare\" sono escluse: aggiungi --include-review per rinominarle.";
pub const NOTHING_TO_RENAME: &str = "Nessun file da rinominare.";
pub const NOTHING_TO_UNDO: &str = "Non c'è nessuna operazione da annullare.";
pub const NO_SERIES: &str = "Nessuna serie trovata.";
pub const OTHER_RESULTS: &str = "Altri risultati (scegli con --choice N):";
pub const KEY_PROMPT: &str = "Incolla la chiave e premi Invio: ";
pub const KEY_EMPTY: &str = "Nessuna chiave inserita.";
pub const KEY_DELETED: &str = "Chiave cancellata.";
pub const KEY_VALID: &str = "Chiave valida.";
pub const KEY_MISSING: &str = "Nessuna chiave salvata per questo database.";
pub const UNKNOWN_TOKENS: &str = "Token non riconosciuti nel formato:";

pub fn provider(id: ProviderId) -> &'static str {
    match id {
        ProviderId::Tmdb => "TMDB",
        ProviderId::Tvdb => "TVDB",
    }
}

pub fn missing_key(id: ProviderId) -> String {
    format!(
        "manca la chiave API di {}: salvala con `renbot keys set {}` o dalle Impostazioni",
        provider(id),
        provider(id).to_lowercase()
    )
}

pub fn key_saved(source: KeySource, key: ApiKey) -> String {
    match source {
        KeySource::Keychain => "Chiave salvata nel portachiavi di sistema.".to_string(),
        KeySource::File => {
            "Chiave salvata nel file di configurazione (portachiavi non disponibile).".to_string()
        }
        KeySource::Environment => format!(
            "Chiave letta dalla variabile d'ambiente {}.",
            key.env_var().unwrap_or_default()
        ),
    }
}

pub fn summary(total: usize, ready: usize, review: usize, other: usize) -> String {
    format!("{total} file · {ready} pronti · {review} da verificare · {other} non rinominabili")
}

pub fn renamed(renamed: usize, failed: usize) -> String {
    match failed {
        0 => format!("Rinominati {renamed} file. Per tornare indietro: renbot undo"),
        n => format!("Rinominati {renamed} file, {n} non riusciti."),
    }
}

pub fn undone(restored: usize, failed: usize) -> String {
    match failed {
        0 => format!("Ripristinati {restored} file."),
        n => format!("Ripristinati {restored} file, {n} non ripristinabili."),
    }
}

pub fn list_summary(rows: usize, ready: usize) -> String {
    format!("{rows} righe · {ready} pronte")
}
