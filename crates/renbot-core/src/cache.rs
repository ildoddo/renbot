//! Cache su disco delle risposte dei provider.

use std::fs;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use directories::ProjectDirs;
use serde::{Deserialize, Serialize};

/// Durata predefinita di una voce: 24 ore.
pub const DEFAULT_TTL: Duration = Duration::from_secs(24 * 60 * 60);

#[derive(Debug, Clone)]
pub struct Cache {
    dir: PathBuf,
    ttl: Duration,
}

#[derive(Serialize, Deserialize)]
struct Entry {
    key: String,
    stored_at: u64,
    body: String,
}

impl Cache {
    pub fn new(dir: impl Into<PathBuf>, ttl: Duration) -> Self {
        Self {
            dir: dir.into(),
            ttl,
        }
    }

    /// Cache nella cartella prevista dal sistema operativo, con durata di 24 ore.
    pub fn in_default_dir() -> Option<Self> {
        let dirs = ProjectDirs::from("", "", "renbot")?;
        Some(Self::new(dirs.cache_dir(), DEFAULT_TTL))
    }

    pub fn get(&self, key: &str) -> Option<String> {
        self.get_at(key, now())
    }

    pub fn put(&self, key: &str, body: &str) -> io::Result<()> {
        self.put_at(key, body, now())
    }

    /// Elimina tutte le voci. Una cartella di cache inesistente non è un errore.
    pub fn clear(&self) -> io::Result<()> {
        let entries = match fs::read_dir(&self.dir) {
            Ok(entries) => entries,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        for entry in entries {
            let path = entry?.path();
            if path.extension().is_some_and(|e| e == "json") {
                fs::remove_file(path)?;
            }
        }
        Ok(())
    }

    fn get_at(&self, key: &str, now: u64) -> Option<String> {
        let text = fs::read_to_string(self.path(key)).ok()?;
        let entry: Entry = serde_json::from_str(&text).ok()?;
        let fresh = now.saturating_sub(entry.stored_at) < self.ttl.as_secs();
        (entry.key == key && fresh).then_some(entry.body)
    }

    fn put_at(&self, key: &str, body: &str, now: u64) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        let entry = Entry {
            key: key.to_string(),
            stored_at: now,
            body: body.to_string(),
        };
        fs::write(self.path(key), serde_json::to_string(&entry)?)
    }

    fn path(&self, key: &str) -> PathBuf {
        self.dir.join(format!("{:016x}.json", fnv1a(key)))
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Hash FNV-1a a 64 bit: stabile tra versioni del compilatore, a differenza di quello di `std`.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempDir;

    #[test]
    fn salva_e_rilegge() {
        let dir = TempDir::new("cache-roundtrip");
        let cache = Cache::new(dir.path(), DEFAULT_TTL);
        assert_eq!(cache.get("chiave"), None);
        cache.put("chiave", "{\"a\":1}").unwrap();
        assert_eq!(cache.get("chiave").as_deref(), Some("{\"a\":1}"));
        assert_eq!(cache.get("altra"), None);
    }

    #[test]
    fn scade_dopo_24_ore() {
        let dir = TempDir::new("cache-ttl");
        let cache = Cache::new(dir.path(), DEFAULT_TTL);
        cache.put_at("chiave", "corpo", 1_000).unwrap();
        let day = DEFAULT_TTL.as_secs();
        assert_eq!(
            cache.get_at("chiave", 1_000 + day - 1).as_deref(),
            Some("corpo")
        );
        assert_eq!(cache.get_at("chiave", 1_000 + day), None);
    }

    #[test]
    fn svuota_la_cache() {
        let dir = TempDir::new("cache-clear");
        let cache = Cache::new(dir.path().join("cache"), DEFAULT_TTL);
        cache.clear().unwrap();
        cache.put("a", "1").unwrap();
        cache.put("b", "2").unwrap();
        cache.clear().unwrap();
        assert_eq!(cache.get("a"), None);
        assert_eq!(cache.get("b"), None);
    }

    #[test]
    fn hash_stabile() {
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
    }
}
