//! Chiavi API: portachiavi del sistema operativo, con ripiego su file e variabili d'ambiente.
//!
//! I valori delle chiavi non vengono mai scritti nei log né nei messaggi di errore.

use std::collections::BTreeMap;
use std::fs;
use std::io::{self, Write};
use std::path::PathBuf;

use super::config_dir;

const SERVICE: &str = "renbot";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ApiKey {
    Tmdb,
    Tvdb,
    Omdb,
}

impl ApiKey {
    pub const ALL: [ApiKey; 3] = [ApiKey::Tmdb, ApiKey::Tvdb, ApiKey::Omdb];

    /// Nome con cui la chiave è salvata.
    pub fn id(self) -> &'static str {
        match self {
            ApiKey::Tmdb => "tmdb",
            ApiKey::Tvdb => "tvdb",
            ApiKey::Omdb => "omdb",
        }
    }

    /// Variabile d'ambiente accettata in alternativa al salvataggio.
    pub fn env_var(self) -> Option<&'static str> {
        match self {
            ApiKey::Tmdb => Some("TMDB_API_KEY"),
            ApiKey::Tvdb => Some("TVDB_API_KEY"),
            ApiKey::Omdb => None,
        }
    }
}

/// Da dove proviene una chiave.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Keychain,
    File,
    Environment,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("archivio delle chiavi non disponibile: {0}")]
pub struct KeyError(String);

impl KeyError {
    fn new(error: impl std::fmt::Display) -> Self {
        Self(error.to_string())
    }
}

pub trait SecretStore: Send + Sync {
    fn get(&self, key: ApiKey) -> Result<Option<String>, KeyError>;
    fn set(&self, key: ApiKey, value: &str) -> Result<(), KeyError>;
    fn delete(&self, key: ApiKey) -> Result<(), KeyError>;
}

/// Portachiavi del sistema operativo.
pub struct KeychainStore;

impl KeychainStore {
    fn entry(key: ApiKey) -> Result<keyring::v1::Entry, KeyError> {
        keyring::v1::Entry::new(SERVICE, key.id()).map_err(KeyError::new)
    }
}

impl SecretStore for KeychainStore {
    fn get(&self, key: ApiKey) -> Result<Option<String>, KeyError> {
        match Self::entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::v1::Error::NoEntry) => Ok(None),
            Err(error) => Err(KeyError::new(error)),
        }
    }

    fn set(&self, key: ApiKey, value: &str) -> Result<(), KeyError> {
        Self::entry(key)?.set_password(value).map_err(KeyError::new)
    }

    fn delete(&self, key: ApiKey) -> Result<(), KeyError> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::v1::Error::NoEntry) => Ok(()),
            Err(error) => Err(KeyError::new(error)),
        }
    }
}

/// File JSON nella cartella di configurazione, leggibile solo dal proprietario dove supportato.
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    fn read(&self) -> Result<BTreeMap<String, String>, KeyError> {
        match fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|_| KeyError::new("file non valido")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(error) => Err(KeyError::new(error)),
        }
    }

    fn write(&self, keys: &BTreeMap<String, String>) -> Result<(), KeyError> {
        let write = || -> io::Result<()> {
            if let Some(parent) = self.path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut options = fs::OpenOptions::new();
            options.write(true).create(true).truncate(true);
            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }
            let mut file = options.open(&self.path)?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                file.set_permissions(fs::Permissions::from_mode(0o600))?;
            }
            file.write_all(serde_json::to_string_pretty(keys)?.as_bytes())
        };
        write().map_err(KeyError::new)
    }
}

impl SecretStore for FileStore {
    fn get(&self, key: ApiKey) -> Result<Option<String>, KeyError> {
        Ok(self.read()?.remove(key.id()))
    }

    fn set(&self, key: ApiKey, value: &str) -> Result<(), KeyError> {
        let mut keys = self.read().unwrap_or_default();
        keys.insert(key.id().to_string(), value.to_string());
        self.write(&keys)
    }

    fn delete(&self, key: ApiKey) -> Result<(), KeyError> {
        let mut keys = self.read()?;
        if keys.remove(key.id()).is_some() {
            self.write(&keys)?;
        }
        Ok(())
    }
}

type EnvLookup = Box<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// Legge e salva le chiavi scegliendo l'archivio disponibile.
pub struct KeyManager {
    keychain: Option<Box<dyn SecretStore>>,
    file: FileStore,
    env: EnvLookup,
}

impl KeyManager {
    pub fn new(keychain: Option<Box<dyn SecretStore>>, file: FileStore, env: EnvLookup) -> Self {
        Self {
            keychain,
            file,
            env,
        }
    }

    /// Portachiavi di sistema, file nella cartella di configurazione e variabili d'ambiente reali.
    pub fn system() -> Option<Self> {
        let file = FileStore::new(config_dir()?.join("keys.json"));
        Some(Self::new(
            Some(Box::new(KeychainStore)),
            file,
            Box::new(|name| std::env::var(name).ok()),
        ))
    }

    /// Chiave e provenienza. Le chiavi salvate hanno la precedenza sulle variabili d'ambiente.
    pub fn get(&self, key: ApiKey) -> Option<(String, KeySource)> {
        let saved = |store: &dyn SecretStore, source| match store.get(key) {
            Ok(value) => value.filter(|v| !v.is_empty()).map(|v| (v, source)),
            Err(error) => {
                tracing::debug!(key = key.id(), %error, "lettura della chiave non riuscita");
                None
            }
        };
        self.keychain
            .as_deref()
            .and_then(|store| saved(store, KeySource::Keychain))
            .or_else(|| saved(&self.file, KeySource::File))
            .or_else(|| {
                let value = (self.env)(key.env_var()?)?;
                let value = value.trim();
                (!value.is_empty()).then(|| (value.to_string(), KeySource::Environment))
            })
    }

    /// Salva la chiave nel portachiavi o, se non è disponibile, nel file.
    pub fn set(&self, key: ApiKey, value: &str) -> Result<KeySource, KeyError> {
        let value = value.trim();
        if let Some(keychain) = &self.keychain {
            match keychain.set(key, value) {
                Ok(()) => {
                    let _ = self.file.delete(key);
                    return Ok(KeySource::Keychain);
                }
                Err(error) => {
                    tracing::warn!(key = key.id(), %error, "portachiavi non disponibile, uso il file");
                }
            }
        }
        self.file.set(key, value)?;
        Ok(KeySource::File)
    }

    /// Cancella la chiave da tutti gli archivi. Una variabile d'ambiente resta valida.
    pub fn delete(&self, key: ApiKey) -> Result<(), KeyError> {
        let keychain = match &self.keychain {
            Some(store) => store.delete(key),
            None => Ok(()),
        };
        let file = self.file.delete(key);
        // Con il portachiavi non raggiungibile basta la cancellazione dal file.
        file.and(match self.keychain.as_deref().map(|s| s.get(key)) {
            Some(Ok(Some(_))) => keychain,
            _ => Ok(()),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use super::*;
    use crate::testing::TempDir;

    /// Portachiavi in memoria: i test non toccano mai quello reale.
    #[derive(Default)]
    struct MemoryStore {
        broken: bool,
        keys: Mutex<HashMap<ApiKey, String>>,
    }

    impl SecretStore for MemoryStore {
        fn get(&self, key: ApiKey) -> Result<Option<String>, KeyError> {
            if self.broken {
                return Err(KeyError::new("non disponibile"));
            }
            Ok(self.keys.lock().unwrap().get(&key).cloned())
        }

        fn set(&self, key: ApiKey, value: &str) -> Result<(), KeyError> {
            if self.broken {
                return Err(KeyError::new("non disponibile"));
            }
            self.keys.lock().unwrap().insert(key, value.to_string());
            Ok(())
        }

        fn delete(&self, key: ApiKey) -> Result<(), KeyError> {
            if self.broken {
                return Err(KeyError::new("non disponibile"));
            }
            self.keys.lock().unwrap().remove(&key);
            Ok(())
        }
    }

    fn manager(dir: &TempDir, keychain: Option<MemoryStore>, env: &[(&str, &str)]) -> KeyManager {
        let env: HashMap<String, String> = env
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        KeyManager::new(
            keychain.map(|store| Box::new(store) as Box<dyn SecretStore>),
            FileStore::new(dir.path().join("keys.json")),
            Box::new(move |name| env.get(name).cloned()),
        )
    }

    #[test]
    fn salva_nel_portachiavi_quando_disponibile() {
        let dir = TempDir::new("keys-keychain");
        let keys = manager(&dir, Some(MemoryStore::default()), &[]);
        assert_eq!(keys.get(ApiKey::Tmdb), None);
        assert_eq!(keys.set(ApiKey::Tmdb, "  abc  "), Ok(KeySource::Keychain));
        assert_eq!(
            keys.get(ApiKey::Tmdb),
            Some(("abc".to_string(), KeySource::Keychain))
        );
        assert!(!dir.path().join("keys.json").exists());

        keys.delete(ApiKey::Tmdb).unwrap();
        assert_eq!(keys.get(ApiKey::Tmdb), None);
    }

    #[test]
    fn ripiega_sul_file_se_il_portachiavi_non_funziona() {
        let dir = TempDir::new("keys-file");
        let broken = MemoryStore {
            broken: true,
            ..MemoryStore::default()
        };
        let keys = manager(&dir, Some(broken), &[]);
        assert_eq!(keys.set(ApiKey::Tvdb, "xyz"), Ok(KeySource::File));
        assert_eq!(
            keys.get(ApiKey::Tvdb),
            Some(("xyz".to_string(), KeySource::File))
        );
        assert_eq!(keys.get(ApiKey::Tmdb), None);

        keys.delete(ApiKey::Tvdb).unwrap();
        assert_eq!(keys.get(ApiKey::Tvdb), None);
    }

    #[cfg(unix)]
    #[test]
    fn il_file_è_leggibile_solo_dal_proprietario() {
        use std::os::unix::fs::PermissionsExt;
        let dir = TempDir::new("keys-permissions");
        let keys = manager(&dir, None, &[]);
        keys.set(ApiKey::Omdb, "k").unwrap();
        let mode = fs::metadata(dir.path().join("keys.json"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[test]
    fn le_variabili_d_ambiente_valgono_in_assenza_di_chiavi_salvate() {
        let dir = TempDir::new("keys-env");
        let env = [("TMDB_API_KEY", "da-ambiente"), ("TVDB_API_KEY", "  ")];
        let keys = manager(&dir, Some(MemoryStore::default()), &env);
        assert_eq!(
            keys.get(ApiKey::Tmdb),
            Some(("da-ambiente".to_string(), KeySource::Environment))
        );
        assert_eq!(keys.get(ApiKey::Tvdb), None);
        assert_eq!(keys.get(ApiKey::Omdb), None);

        keys.set(ApiKey::Tmdb, "salvata").unwrap();
        assert_eq!(
            keys.get(ApiKey::Tmdb),
            Some(("salvata".to_string(), KeySource::Keychain))
        );
    }

    #[test]
    fn gli_errori_non_contengono_il_valore_della_chiave() {
        let dir = TempDir::new("keys-errors");
        // Una cartella al posto del file rende impossibile la scrittura.
        fs::create_dir_all(dir.path().join("keys.json")).unwrap();
        let keys = manager(&dir, None, &[]);
        let error = keys.set(ApiKey::Tmdb, "SEGRETO").unwrap_err();
        assert!(!error.to_string().contains("SEGRETO"));
    }
}
