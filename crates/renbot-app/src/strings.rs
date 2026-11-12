//! Tutti i testi dell'interfaccia, in italiano.

use renbot_core::config::keys::{ApiKey, KeySource};
use renbot_core::config::Theme;
use renbot_core::providers::ProviderId;

pub const APP_NAME: &str = "renbot";

// Barra laterale
pub const NAV_LIBRARY: &str = "Libreria";
pub const NAV_EPISODES: &str = "Episodi";
pub const NAV_LISTS: &str = "Liste";
pub const NAV_SETTINGS: &str = "Impostazioni";

// Azioni comuni
pub const ADD_FILES: &str = "File";
pub const ADD_FOLDER: &str = "Cartella";
pub const CLEAR: &str = "Svuota";
pub const SEARCH: &str = "Cerca";
pub const CANCEL: &str = "Annulla";
pub const CLOSE: &str = "Chiudi";
pub const UNDO_LAST: &str = "Annulla ultima rinomina";
pub const DATABASE: &str = "Database:";
pub const LANGUAGE: &str = "Lingua:";
pub const USE_FALLBACK: &str = "usa l'altro database se non trovo risultati";
pub const SEASON: &str = "Stagione:";
pub const ALL_SEASONS: &str = "Tutte";
pub const VIDEO_FILES: &str = "File video";

// Intestazioni di tabella
pub const COL_STATUS: &str = "Stato";
pub const COL_ORIGINAL: &str = "Nome originale";
pub const COL_NEW: &str = "Nuovo nome";
pub const COL_MATCH: &str = "Corrispondenza";
pub const COL_FILE: &str = "File";
pub const COL_SEASON: &str = "Stagione";
pub const COL_EPISODE: &str = "Episodio";
pub const COL_NUMBER: &str = "Ep.";
pub const COL_SEASON_SHORT: &str = "St.";
pub const COL_TITLE: &str = "Titolo";
pub const COL_DATE: &str = "Data";

// Stati
pub const ST_READY: &str = "Pronto";
pub const ST_REVIEW: &str = "Da verificare";
pub const ST_CONFLICT: &str = "Conflitto";
pub const ST_ERROR: &str = "Errore";
pub const ST_PENDING: &str = "In attesa";
pub const ST_RENAMED: &str = "Rinominato";
pub const ST_UNCHANGED: &str = "Invariato";
pub const ST_NO_MATCH: &str = "Nessun risultato";
pub const ST_NO_EPISODE: &str = "Episodio non trovato";
pub const ST_EXCLUDED: &str = "Escluso";
pub const ST_UNASSIGNED: &str = "Senza episodio";

pub const DETAIL_REVIEW: &str =
    "Il titolo trovato somiglia poco al nome del file: controlla prima di includerlo.";
pub const DETAIL_TARGET_EXISTS: &str =
    "Esiste già un file con il nuovo nome. Non verrà sovrascritto.";
pub const DETAIL_DUPLICATE: &str = "Più file avrebbero lo stesso nuovo nome.";
pub const DETAIL_SOURCE_MISSING: &str = "Il file originale non esiste più.";
pub const DETAIL_INVALID_TARGET: &str = "Il formato non produce un nome di file valido.";
pub const DETAIL_NO_MATCH: &str =
    "Prova l'altro database oppure cerca il titolo a mano dal pannello Corrispondenza.";
pub const DETAIL_NO_EPISODE: &str =
    "La serie è stata trovata ma non contiene questo episodio. Per gli anime usa una lista.";
pub const DETAIL_DUPLICATE_EPISODE: &str = "Lo stesso episodio è assegnato a più righe.";

// Libreria
pub const LIB_EMPTY_TITLE: &str = "Trascina qui file o cartelle";
pub const LIB_EMPTY_HINT: &str = "oppure usa i pulsanti File e Cartella qui sopra";
pub const LIB_REMATCH: &str = "Cerca di nuovo";
pub const LIB_NO_KEY: &str = "Per cercare i titoli serve una chiave API.";
pub const OPEN_SETTINGS: &str = "Apri Impostazioni";
pub const LIB_PANEL: &str = "Corrispondenza";
pub const LIB_FROM_NAME: &str = "Ricavato dal nome del file";
pub const LIB_CANDIDATES: &str = "Candidati";
pub const LIB_USE_THIS: &str = "Usa questo";
pub const LIB_IN_USE: &str = "in uso";
pub const LIB_SEARCH_HINT: &str = "titolo oppure ID IMDb (tt…)";
pub const LIB_NO_CANDIDATES: &str = "Nessun candidato.";
pub const F_TITLE: &str = "Titolo";
pub const F_YEAR: &str = "Anno";
pub const F_EPISODE: &str = "Episodio";
pub const F_QUALITY: &str = "Qualità";
pub const F_GROUP: &str = "Gruppo";
pub const KIND_MOVIE: &str = "Film";
pub const KIND_SERIES: &str = "Serie";

pub fn rename_n(count: usize) -> String {
    match count {
        1 => "Rinomina 1 file".to_string(),
        n => format!("Rinomina {n} file"),
    }
}

pub fn library_summary(total: usize, ready: usize, review: usize, conflicts: usize) -> String {
    format!("{total} file · {ready} pronti · {review} da verificare · {conflicts} conflitti")
}

pub const CONFIRM_TITLE: &str = "Confermi la rinomina?";
pub const CONFIRM_NOTE: &str =
    "I file esistenti non vengono mai sovrascritti. Puoi annullare l'operazione subito dopo.";
pub const CONFIRM_FIRST: &str = "Primo file:";

pub fn confirm_body(count: usize) -> String {
    match count {
        1 => "Sta per essere rinominato 1 file.".to_string(),
        n => format!("Stanno per essere rinominati {n} file."),
    }
}

// Episodi
pub const EP_HINT: &str = "cerca una serie…";
pub const EP_EMPTY_TITLE: &str = "Cerca una serie per vederne gli episodi";
pub const EP_EMPTY_HINT: &str = "Scrivi il titolo qui sopra e premi Cerca";
pub const EP_RESULTS: &str = "Risultati";
pub const EP_PICK: &str = "Scegli una serie dall'elenco.";
pub const EP_NEW_LIST: &str = "Crea lista";
pub const EP_REFRESH: &str = "Aggiorna";
pub const EP_REFRESH_HINT: &str = "Svuota la cache e scarica di nuovo i dati";
pub const EP_NO_RESULTS: &str = "Nessuna serie trovata.";
pub const LOADING: &str = "Caricamento…";

pub fn episode_count(episodes: usize, seasons: usize) -> String {
    format!("{seasons} stagioni · {episodes} episodi")
}

// Liste
pub const LIST_EMPTY_TITLE: &str = "Nessuna lista aperta";
pub const LIST_EMPTY_HINT: &str = "Cerca una serie qui sopra oppure apri una lista salvata";
pub const LIST_OPEN: &str = "Apri…";
pub const LIST_SAVE: &str = "Salva…";
pub const LIST_CHANGE: &str = "Cambia serie";
pub const LIST_START: &str = "Inizia dall'episodio:";
pub const LIST_AUTO: &str = "Assegna in ordine";
pub const LIST_AUTO_HINT: &str =
    "Assegna gli episodi della stagione, in ordine, alle righe incluse che non ne hanno ancora uno";
pub const LIST_RESET: &str = "Azzera";
pub const LIST_RESET_HINT: &str = "Toglie l'episodio da tutte le righe";
pub const LIST_NO_FILES: &str = "Aggiungi i file trascinandoli qui o con il pulsante File.";
pub const LIST_NONE: &str = "—";
pub const LIST_FILE_TYPE: &str = "Lista renbot";
pub const LIST_DEFAULT_NAME: &str = "lista.json";
pub const MOVE_UP: &str = "Sposta su";
pub const MOVE_DOWN: &str = "Sposta giù";

pub fn apply_list(count: usize) -> String {
    format!("Applica lista ({count})")
}

pub fn list_summary(rows: usize, confirmed: usize) -> String {
    format!("{rows} righe · {confirmed} confermate")
}

// Impostazioni
pub const SET_KEYS: &str = "Chiavi API";
pub const SET_KEYS_HINT: &str =
    "Le chiavi sono salvate nel portachiavi del sistema operativo e non compaiono mai nei log.";
pub const SET_KEY_HINT: &str = "incolla qui la chiave";
pub const SET_SHOW: &str = "Mostra";
pub const SET_HIDE: &str = "Nascondi";
pub const SET_SAVE: &str = "Salva";
pub const SET_VERIFY: &str = "Verifica chiave";
pub const SET_DELETE: &str = "Cancella";
pub const SET_VERIFYING: &str = "Verifica in corso…";
pub const SET_KEY_OK: &str = "Chiave valida";
pub const SET_NO_KEY: &str = "Nessuna chiave salvata";
pub const SET_OPTIONAL: &str = "facoltativa";
pub const SET_FORMATS: &str = "Formato dei nomi";
pub const SET_FORMAT_HINT: &str =
    "La barra / separa le cartelle. Fai clic su un token per aggiungerlo al campo selezionato.";
pub const SET_MOVIE_FORMAT: &str = "Film";
pub const SET_SERIES_FORMAT: &str = "Serie";
pub const SET_PRESET: &str = "Schema pronto:";
pub const SET_PRESET_PICK: &str = "scegli…";
pub const SET_RESET_FORMAT: &str = "Ripristina predefinito";
pub const SET_PREVIEW: &str = "Anteprima:";
pub const SET_UNKNOWN_TOKENS: &str = "Token non riconosciuti:";
pub const SET_SEARCH: &str = "Ricerca";
pub const SET_APPEARANCE: &str = "Aspetto";
pub const SET_THEME: &str = "Tema:";
pub const SET_LOG: &str = "Registro";
pub const SET_LOG_HINT: &str = "File con i messaggi dell'ultima esecuzione, utile per segnalare un problema. Non contiene mai le chiavi API.";
pub const PRESETS: [&str; 3] = ["Con cartelle", "Solo nome del file", "Con qualità"];
pub const SAMPLE_SERIES: &str = "Nome della serie";
pub const SAMPLE_EPISODE: &str = "Titolo dell'episodio";
pub const SAMPLE_MOVIE: &str = "Titolo del film";

pub const TOKENS: [(&str, &str); 9] = [
    ("{n}", "titolo del film o della serie"),
    ("{y}", "anno"),
    ("{s}", "stagione"),
    ("{e}", "episodio"),
    ("{s00e00}", "stagione ed episodio, es. S01E02"),
    ("{t}", "titolo dell'episodio"),
    ("{q}", "qualità, es. 1080p WEB-DL"),
    ("{g}", "gruppo di release"),
    ("{ext}", "estensione"),
];

pub fn provider(id: ProviderId) -> &'static str {
    match id {
        ProviderId::Tmdb => "TMDB",
        ProviderId::Tvdb => "TVDB",
    }
}

pub fn key_name(key: ApiKey) -> &'static str {
    match key {
        ApiKey::Tmdb => "TheMovieDB (TMDB)",
        ApiKey::Tvdb => "TheTVDB (TVDB)",
        ApiKey::Omdb => "OMDb",
    }
}

pub fn key_help(key: ApiKey) -> &'static str {
    match key {
        ApiKey::Tmdb => {
            "Film, serie e anime. Va bene sia la chiave API v3 sia il token di lettura v4."
        }
        ApiKey::Tvdb => "Serie e anime. Serve una chiave API v4.",
        ApiKey::Omdb => "Per ora viene solo salvata e verificata.",
    }
}

pub fn key_source(source: KeySource, key: ApiKey) -> String {
    match source {
        KeySource::Keychain => "Salvata nel portachiavi di sistema".to_string(),
        KeySource::File => {
            "Salvata nel file di configurazione (portachiavi non disponibile)".to_string()
        }
        KeySource::Environment => format!(
            "In uso dalla variabile d'ambiente {}",
            key.env_var().unwrap_or_default()
        ),
    }
}

pub fn missing_key(id: ProviderId) -> String {
    format!(
        "Manca la chiave API di {}: inseriscila nelle Impostazioni.",
        provider(id)
    )
}

pub fn language(code: &str) -> &str {
    match code {
        "it" => "Italiano",
        "en" => "Inglese",
        "de" => "Tedesco",
        "fr" => "Francese",
        "es" => "Spagnolo",
        "pt" => "Portoghese",
        "ja" => "Giapponese",
        other => other,
    }
}

pub fn theme(theme: Theme) -> &'static str {
    match theme {
        Theme::System => "Come il sistema",
        Theme::Light => "Chiaro",
        Theme::Dark => "Scuro",
    }
}

// Messaggi di stato
pub const MSG_KEY_SAVED: &str = "Chiave salvata.";
pub const MSG_KEY_DELETED: &str = "Chiave cancellata.";
pub const MSG_LIST_SAVED: &str = "Lista salvata.";
pub const MSG_LIST_LOADED: &str = "Lista caricata.";
pub const MSG_CACHE_CLEARED: &str = "Cache svuotata.";
pub const MSG_NO_VIDEO: &str = "Nessun file video trovato.";
pub const MSG_NOTHING_TO_UNDO: &str = "Non c'è nessuna operazione da annullare.";

pub fn msg_added(count: usize) -> String {
    format!("Aggiunti {count} file.")
}

pub fn msg_matching(done: usize, total: usize) -> String {
    format!("Ricerca dei titoli: {done} di {total}")
}

pub fn msg_renamed(renamed: usize, failed: usize) -> String {
    match failed {
        0 => format!("Rinominati {renamed} file."),
        n => format!("Rinominati {renamed} file, {n} non riusciti."),
    }
}

pub fn msg_undone(restored: usize, failed: usize) -> String {
    match failed {
        0 => format!("Ripristinati {restored} file."),
        n => format!("Ripristinati {restored} file, {n} non ripristinabili."),
    }
}

pub fn msg_error(error: impl std::fmt::Display) -> String {
    format!("Errore: {error}")
}
