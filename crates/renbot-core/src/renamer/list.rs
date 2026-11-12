//! Liste di rinomine: più file di una serie, ciascuno con l'episodio assegnato.

use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::format::{render, FormatValues};
use super::RenameItem;
use crate::parser;
use crate::providers::{Episode, SearchResult};

const FILE_VERSION: u32 = 1;

/// Riferimento a un episodio dell'elenco della serie.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EpisodeRef {
    pub season: u32,
    pub number: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ListRow {
    pub file: PathBuf,
    pub episode: Option<EpisodeRef>,
    /// Le righe escluse restano nella lista ma non vengono rinominate.
    pub included: bool,
}

/// Lista di rinomine per una serie. Contiene anche l'elenco degli episodi,
/// così una lista salvata si riapre senza interrogare di nuovo il database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RenameList {
    pub series: SearchResult,
    pub language: String,
    pub episodes: Vec<Episode>,
    pub rows: Vec<ListRow>,
}

#[derive(Serialize)]
struct FileRef<'a> {
    version: u32,
    list: &'a RenameList,
}

#[derive(Deserialize)]
struct File {
    version: u32,
    list: RenameList,
}

impl RenameList {
    pub fn new(series: SearchResult, language: impl Into<String>, episodes: Vec<Episode>) -> Self {
        Self {
            series,
            language: language.into(),
            episodes,
            rows: Vec::new(),
        }
    }

    /// Aggiunge in coda i file non ancora presenti, in ordine naturale di nome
    /// (`t2` prima di `t10`).
    pub fn add_files(&mut self, files: impl IntoIterator<Item = PathBuf>) {
        let mut new: Vec<PathBuf> = files
            .into_iter()
            .filter(|file| !self.rows.iter().any(|row| &row.file == file))
            .collect();
        new.sort_by(|a, b| natural_cmp(a, b));
        new.dedup();
        self.rows.extend(new.into_iter().map(|file| ListRow {
            file,
            episode: None,
            included: true,
        }));
    }

    /// Stagioni presenti nell'elenco degli episodi, in ordine.
    pub fn seasons(&self) -> Vec<u32> {
        let mut seasons: Vec<u32> = self.episodes.iter().map(|e| e.season).collect();
        seasons.sort_unstable();
        seasons.dedup();
        seasons
    }

    pub fn episode(&self, reference: EpisodeRef) -> Option<&Episode> {
        self.episodes
            .iter()
            .find(|e| e.season == reference.season && e.number == reference.number)
    }

    /// Assegna alle righe indicate, nell'ordine della lista, gli episodi consecutivi
    /// della stagione a partire da `first_episode`. Le righe escluse non sono toccate;
    /// quelle oltre la fine della stagione restano senza episodio.
    pub fn auto_assign(&mut self, rows: &[usize], season: u32, first_episode: u32) {
        let mut available: Vec<u32> = self
            .episodes
            .iter()
            .filter(|e| e.season == season && e.number >= first_episode)
            .map(|e| e.number)
            .collect();
        available.sort_unstable();
        let mut available = available.into_iter();

        let mut indexes = rows.to_vec();
        indexes.sort_unstable();
        indexes.dedup();
        for index in indexes {
            if let Some(row) = self.rows.get_mut(index).filter(|row| row.included) {
                row.episode = available.next().map(|number| EpisodeRef { season, number });
            }
        }
    }

    /// Assegnazione manuale di una riga; `None` toglie l'episodio.
    pub fn assign(&mut self, row: usize, episode: Option<EpisodeRef>) {
        if let Some(row) = self.rows.get_mut(row) {
            row.episode = episode;
        }
    }

    pub fn set_included(&mut self, row: usize, included: bool) {
        if let Some(row) = self.rows.get_mut(row) {
            row.included = included;
        }
    }

    /// Sposta una riga in un'altra posizione.
    pub fn move_row(&mut self, from: usize, to: usize) {
        if from < self.rows.len() && to < self.rows.len() {
            let row = self.rows.remove(from);
            self.rows.insert(to, row);
        }
    }

    /// Righe incluse che condividono lo stesso episodio con un'altra riga.
    pub fn duplicate_rows(&self) -> Vec<usize> {
        let mut count: HashMap<EpisodeRef, usize> = HashMap::new();
        for episode in self.confirmed().filter_map(|(_, row)| row.episode) {
            *count.entry(episode).or_default() += 1;
        }
        self.confirmed()
            .filter(|(_, row)| row.episode.is_some_and(|e| count[&e] > 1))
            .map(|(index, _)| index)
            .collect()
    }

    fn confirmed(&self) -> impl Iterator<Item = (usize, &ListRow)> {
        self.rows.iter().enumerate().filter(|(_, row)| row.included)
    }

    /// Rinomina di una riga, se è inclusa e ha un episodio assegnato.
    pub fn row_item(&self, index: usize, format: &str, base: Option<&Path>) -> Option<RenameItem> {
        let row = self.rows.get(index).filter(|row| row.included)?;
        let episode = self.episode(row.episode?)?;
        let name = row.file.file_name()?.to_string_lossy();
        let values = FormatValues::episode(&parser::parse(&name), &self.series, episode);
        Some(RenameItem::new(&row.file, base, &render(format, &values)))
    }

    /// Rinomine per le righe incluse con un episodio assegnato, da passare all'anteprima.
    pub fn rename_items(&self, format: &str, base: Option<&Path>) -> Vec<RenameItem> {
        (0..self.rows.len())
            .filter_map(|index| self.row_item(index, format, base))
            .collect()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        let file = FileRef {
            version: FILE_VERSION,
            list: self,
        };
        fs::write(path, serde_json::to_string_pretty(&file)?)
    }

    pub fn load(path: &Path) -> io::Result<Self> {
        let file: File = serde_json::from_str(&fs::read_to_string(path)?)?;
        if file.version != FILE_VERSION {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("versione della lista non supportata: {}", file.version),
            ));
        }
        Ok(file.list)
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
enum Chunk {
    Number(u64),
    Text(String),
}

/// Ordine alfabetico in cui le sequenze di cifre contano come numeri.
fn natural_cmp(a: &Path, b: &Path) -> Ordering {
    natural_key(a).cmp(&natural_key(b)).then_with(|| a.cmp(b))
}

fn natural_key(path: &Path) -> Vec<Chunk> {
    let text = path.to_string_lossy().to_lowercase();
    let mut chunks = Vec::new();
    let mut current = String::new();
    let mut digits = false;
    for c in text.chars() {
        if c.is_ascii_digit() != digits && !current.is_empty() {
            chunks.push(chunk(std::mem::take(&mut current), digits));
        }
        digits = c.is_ascii_digit();
        current.push(c);
    }
    if !current.is_empty() {
        chunks.push(chunk(current, digits));
    }
    chunks
}

fn chunk(text: String, digits: bool) -> Chunk {
    if digits {
        Chunk::Number(text.parse().unwrap_or(u64::MAX))
    } else {
        Chunk::Text(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::MediaKind;
    use crate::providers::ProviderId;
    use crate::renamer::format::DEFAULT_SERIES_FORMAT;
    use crate::testing::TempDir;

    fn episode(season: u32, number: u32) -> Episode {
        Episode {
            season,
            number,
            title: format!("Titolo {season}.{number}"),
            air_date: Some("2020-01-01".to_string()),
        }
    }

    /// Serie con 4 episodi nella prima stagione e 2 nella seconda; 5 file da disco.
    fn list() -> RenameList {
        let series = SearchResult {
            provider: ProviderId::Tvdb,
            id: "42".to_string(),
            kind: MediaKind::Episode,
            title: "Show Name".to_string(),
            original_title: Some("Nome originale".to_string()),
            year: Some(2019),
        };
        let episodes = vec![
            episode(1, 1),
            episode(1, 2),
            episode(1, 3),
            episode(1, 4),
            episode(2, 1),
            episode(2, 2),
        ];
        let mut list = RenameList::new(series, "it", episodes);
        list.add_files(
            ["t10.mkv", "t2.mkv", "t1.mkv", "t3.mkv", "t4.mkv"]
                .map(|name| Path::new("disco").join(name)),
        );
        list
    }

    fn assigned(list: &RenameList) -> Vec<Option<(u32, u32)>> {
        list.rows
            .iter()
            .map(|row| row.episode.map(|e| (e.season, e.number)))
            .collect()
    }

    fn names(list: &RenameList) -> Vec<String> {
        list.rows
            .iter()
            .map(|row| row.file.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn i_file_sono_aggiunti_in_ordine_naturale_e_senza_doppioni() {
        let mut list = list();
        assert_eq!(
            names(&list),
            ["t1.mkv", "t2.mkv", "t3.mkv", "t4.mkv", "t10.mkv"]
        );
        list.add_files([Path::new("disco").join("t2.mkv")]);
        assert_eq!(list.rows.len(), 5);
        assert_eq!(list.seasons(), [1, 2]);
    }

    #[test]
    fn assegnazione_automatica_con_offset() {
        let mut list = list();
        list.auto_assign(&[0, 1, 2, 3, 4], 1, 2);
        assert_eq!(
            assigned(&list),
            [Some((1, 2)), Some((1, 3)), Some((1, 4)), None, None]
        );
    }

    #[test]
    fn più_stagioni_nella_stessa_lista() {
        let mut list = list();
        list.auto_assign(&[2, 0, 1], 1, 1);
        list.auto_assign(&[3, 4], 2, 1);
        assert_eq!(
            assigned(&list),
            [
                Some((1, 1)),
                Some((1, 2)),
                Some((1, 3)),
                Some((2, 1)),
                Some((2, 2))
            ]
        );
    }

    #[test]
    fn riassegnazione_manuale_esclusione_e_riordino() {
        let mut list = list();
        list.set_included(1, false);
        list.auto_assign(&[0, 1, 2], 1, 1);
        assert_eq!(
            assigned(&list)[..3],
            [Some((1, 1)), None, Some((1, 2))],
            "la riga esclusa non riceve episodi"
        );

        list.assign(
            2,
            Some(EpisodeRef {
                season: 2,
                number: 2,
            }),
        );
        list.assign(0, None);
        assert_eq!(assigned(&list)[..3], [None, None, Some((2, 2))]);

        list.move_row(4, 0);
        assert_eq!(names(&list)[0], "t10.mkv");
        assert_eq!(names(&list)[1], "t1.mkv");
    }

    #[test]
    fn segnala_gli_episodi_assegnati_due_volte() {
        let mut list = list();
        let first = Some(EpisodeRef {
            season: 1,
            number: 1,
        });
        list.assign(0, first);
        list.assign(3, first);
        list.assign(
            4,
            Some(EpisodeRef {
                season: 1,
                number: 2,
            }),
        );
        assert_eq!(list.duplicate_rows(), [0, 3]);
        list.set_included(3, false);
        assert!(list.duplicate_rows().is_empty());
    }

    #[test]
    fn produce_le_rinomine_delle_sole_righe_confermate() {
        let mut list = list();
        list.auto_assign(&[0, 1, 2], 1, 1);
        list.set_included(1, false);
        let items = list.rename_items(DEFAULT_SERIES_FORMAT, Some(Path::new("libreria")));
        let targets: Vec<&Path> = items.iter().map(|i| i.target.as_path()).collect();
        let season = Path::new("libreria").join("Show Name").join("Season 1");
        assert_eq!(
            targets,
            [
                season.join("Show Name - S01E01 - Titolo 1.1.mkv"),
                season.join("Show Name - S01E03 - Titolo 1.3.mkv"),
            ]
        );
        assert_eq!(items[0].source, Path::new("disco").join("t1.mkv"));
    }

    #[test]
    fn salvataggio_e_ricaricamento_senza_perdita_di_dati() {
        let dir = TempDir::new("list");
        let path = dir.path().join("lista.json");
        let mut list = list();
        list.auto_assign(&[0, 1, 2, 3, 4], 1, 3);
        list.assign(
            3,
            Some(EpisodeRef {
                season: 2,
                number: 1,
            }),
        );
        list.set_included(4, false);
        list.move_row(0, 2);

        list.save(&path).unwrap();
        let loaded = RenameList::load(&path).unwrap();

        assert_eq!(loaded, list);
        assert_eq!(
            loaded.rename_items(DEFAULT_SERIES_FORMAT, None),
            list.rename_items(DEFAULT_SERIES_FORMAT, None)
        );
    }

    #[test]
    fn rifiuta_una_versione_sconosciuta() {
        let dir = TempDir::new("list-version");
        let path = dir.path().join("lista.json");
        list().save(&path).unwrap();
        let text = fs::read_to_string(&path)
            .unwrap()
            .replace("\"version\": 1", "\"version\": 99");
        fs::write(&path, text).unwrap();
        let error = RenameList::load(&path).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::InvalidData);
    }
}
