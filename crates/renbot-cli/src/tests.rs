//! Test dei comandi con database simulati, chiavi su file temporaneo e cartelle temporanee.

use std::fs;
use std::io::Cursor;

use clap::CommandFactory;
use renbot_core::config::keys::FileStore;
use renbot_core::providers::{Episode, SearchResult};
use renbot_core::renamer::list::EpisodeRef;
use renbot_core::testing::{FakeProvider, TempDir};

use super::*;

struct FakeBackend {
    providers: HashMap<ProviderId, Provider>,
}

impl Backend for FakeBackend {
    fn provider(&self, id: ProviderId) -> Result<Provider, String> {
        self.providers
            .get(&id)
            .cloned()
            .ok_or_else(|| text::missing_key(id))
    }

    fn verify_key<'a>(&'a self, _key: ApiKey, value: &'a str) -> BoxFuture<'a, Result<(), String>> {
        Box::pin(async move {
            if value == "chiave-valida" {
                Ok(())
            } else {
                Err("chiave API non valida".to_string())
            }
        })
    }
}

/// Ambiente di prova: una cartella `video` con alcuni file e nessun portachiavi reale.
struct Fixture {
    dir: TempDir,
    keys: KeyManager,
    backend: FakeBackend,
}

impl Fixture {
    /// TMDB conosce una serie di tre episodi e un film; TVDB non è configurato.
    fn new(name: &str) -> Self {
        let tmdb = FakeProvider::new(ProviderId::Tmdb)
            .with_result(MediaKind::Episode, "1", "Show Name", 2019)
            .with_result(MediaKind::Movie, "9", "Inception", 2010)
            .with_episode(1, 1, "Pilota")
            .with_episode(1, 2, "Secondo")
            .with_episode(2, 1, "Ritorno");
        Self::with_providers(name, vec![tmdb])
    }

    fn with_providers(name: &str, providers: Vec<FakeProvider>) -> Self {
        let dir = TempDir::new(name);
        fs::create_dir_all(dir.path().join("video")).unwrap();
        let keys = KeyManager::new(
            None,
            FileStore::new(dir.path().join("keys.json")),
            Box::new(|_| None),
        );
        let providers = providers
            .into_iter()
            .map(|provider| (provider.id, Arc::new(provider) as Provider))
            .collect();
        Self {
            dir,
            keys,
            backend: FakeBackend { providers },
        }
    }

    fn video(&self, name: &str) -> PathBuf {
        let path = self.dir.path().join("video").join(name);
        fs::write(&path, name).unwrap();
        path
    }

    fn exists(&self, parts: &[&str]) -> bool {
        let relative: PathBuf = parts.iter().collect();
        self.dir.path().join("video").join(relative).is_file()
    }

    /// Esegue `renbot <args>` e restituisce l'esito con il testo stampato.
    async fn run(&self, args: &[&str], input: &str) -> (anyhow::Result<bool>, String) {
        let cli = Cli::try_parse_from(std::iter::once("renbot").chain(args.iter().copied()))
            .expect("argomenti validi");
        let app = App {
            settings: Settings::default(),
            keys: &self.keys,
            backend: &self.backend,
            undo_log: self.dir.path().join("undo.json"),
        };
        let mut out = Vec::new();
        let result = run(cli.command, &app, &mut out, &mut Cursor::new(input)).await;
        (result, String::from_utf8(out).unwrap())
    }

    fn videos(&self) -> String {
        self.dir.path().join("video").display().to_string()
    }
}

const RENAMED_FIRST: [&str; 3] = ["Show Name", "Season 1", "Show Name - S01E01 - Pilota.mkv"];

#[test]
fn la_definizione_dei_comandi_è_coerente() {
    Cli::command().debug_assert();
}

#[test]
fn senza_argomenti_si_apre_la_finestra() {
    assert!(!wants_cli(&["renbot".into()]));
    assert!(wants_cli(&["renbot".into(), "undo".into()]));
}

#[tokio::test]
async fn rename_mostra_solo_l_anteprima() {
    let fx = Fixture::new("cli-preview");
    fx.video("Show.Name.S01E01.1080p.WEB-DL.mkv");
    fx.video("Inception.2010.mkv");
    fx.video("Show.Name.S09E09.mkv");

    let (result, out) = fx.run(&["rename", &fx.videos()], "").await;

    assert!(result.unwrap());
    assert!(out.contains("[Pronto]"), "{out}");
    assert!(out.contains("Show Name - S01E01 - Pilota.mkv"), "{out}");
    assert!(out.contains("Inception (2010).mkv"), "{out}");
    assert!(out.contains("[Episodio non trovato]"), "{out}");
    assert!(
        out.contains("3 file · 2 pronti · 0 da verificare · 1 non rinominabili"),
        "{out}"
    );
    assert!(out.contains("--apply"), "{out}");
    assert!(fx.exists(&["Show.Name.S01E01.1080p.WEB-DL.mkv"]));
    assert!(!fx.exists(&RENAMED_FIRST));
    assert!(!fx.dir.path().join("undo.json").exists());
}

#[tokio::test]
async fn rename_apply_e_undo() {
    let fx = Fixture::new("cli-apply");
    fx.video("Show.Name.S01E01.mkv");
    fx.video("Inception.2010.mkv");

    let (result, out) = fx.run(&["rename", &fx.videos(), "--apply"], "").await;
    assert!(result.unwrap());
    assert!(out.contains("Rinominati 2 file"), "{out}");
    assert!(fx.exists(&RENAMED_FIRST));
    assert!(fx.exists(&["Inception (2010)", "Inception (2010).mkv"]));
    assert!(!fx.exists(&["Show.Name.S01E01.mkv"]));

    let (result, out) = fx.run(&["undo"], "").await;
    assert!(result.unwrap());
    assert!(out.contains("Ripristinati 2 file"), "{out}");
    assert!(fx.exists(&["Show.Name.S01E01.mkv"]));
    assert!(fx.exists(&["Inception.2010.mkv"]));
    assert!(!fx.dir.path().join("video").join("Show Name").exists());

    let (result, out) = fx.run(&["undo"], "").await;
    assert!(result.unwrap());
    assert!(out.contains("nessuna operazione da annullare"), "{out}");
}

#[tokio::test]
async fn gli_abbinamenti_dubbi_sono_esclusi_salvo_richiesta() {
    let fx = Fixture::new("cli-review");
    fx.video("Tutt.Altro.Titolo.S01E01.mkv");

    let (result, out) = fx.run(&["rename", &fx.videos(), "--apply"], "").await;
    assert!(result.unwrap());
    assert!(out.contains("[Da verificare]"), "{out}");
    assert!(out.contains("--include-review"), "{out}");
    assert!(out.contains("Nessun file da rinominare"), "{out}");
    assert!(fx.exists(&["Tutt.Altro.Titolo.S01E01.mkv"]));

    let args = ["rename", &fx.videos(), "--apply", "--include-review"];
    let (result, out) = fx.run(&args, "").await;
    assert!(result.unwrap());
    assert!(out.contains("Rinominati 1 file"), "{out}");
    assert!(fx.exists(&RENAMED_FIRST));
}

#[tokio::test]
async fn non_sovrascrive_un_file_esistente() {
    let fx = Fixture::new("cli-conflict");
    let source = fx.video("Show.Name.S01E01.mkv");
    let target = fx
        .dir
        .path()
        .join("video")
        .join("Show Name")
        .join("Season 1");
    fs::create_dir_all(&target).unwrap();
    let existing = target.join(RENAMED_FIRST[2]);
    fs::write(&existing, "ORIGINALE").unwrap();

    let args = ["rename", source.to_str().unwrap(), "--apply"];
    let (result, out) = fx.run(&args, "").await;
    assert!(result.unwrap());
    assert!(out.contains("[Conflitto]"), "{out}");
    assert!(fx.exists(&["Show.Name.S01E01.mkv"]));
    assert_eq!(fs::read_to_string(&existing).unwrap(), "ORIGINALE");
}

#[tokio::test]
async fn una_cartella_già_ordinata_resta_invariata() {
    let fx = Fixture::new("cli-idempotent");
    fx.video("Show.Name.S01E01.mkv");
    fx.run(&["rename", &fx.videos(), "--apply"], "")
        .await
        .0
        .unwrap();
    assert!(fx.exists(&RENAMED_FIRST));

    let (result, out) = fx.run(&["rename", &fx.videos(), "--apply"], "").await;
    assert!(result.unwrap());
    assert!(out.contains("[Invariato]"), "{out}");
    assert!(out.contains("Nessun file da rinominare"), "{out}");
    assert!(fx.exists(&RENAMED_FIRST));
}

#[tokio::test]
async fn formato_dalla_riga_di_comando() {
    let fx = Fixture::new("cli-format");
    fx.video("Show.Name.S01E02.mkv");
    let args = [
        "rename",
        &fx.videos(),
        "--apply",
        "--series-format",
        "{n} {s00e00} {boh}.{ext}",
    ];
    let (result, out) = fx.run(&args, "").await;
    assert!(result.unwrap());
    assert!(
        out.contains("Token non riconosciuti nel formato: {boh}"),
        "{out}"
    );
    assert!(fx.exists(&["Show Name S01E02 {boh}.mkv"]));
}

#[tokio::test]
async fn database_di_riserva_e_opzione_no_fallback() {
    let tvdb = FakeProvider::new(ProviderId::Tvdb)
        .with_result(MediaKind::Episode, "7", "Show Name", 2019)
        .with_episode(1, 1, "Da TVDB");
    let fx = Fixture::with_providers(
        "cli-fallback",
        vec![FakeProvider::new(ProviderId::Tmdb), tvdb],
    );
    fx.video("Show.Name.S01E01.mkv");

    let (_, out) = fx.run(&["rename", &fx.videos()], "").await;
    assert!(out.contains("Show Name - S01E01 - Da TVDB.mkv"), "{out}");

    let (_, out) = fx.run(&["rename", &fx.videos(), "--no-fallback"], "").await;
    assert!(out.contains("[Nessun risultato]"), "{out}");

    let (_, out) = fx
        .run(
            &["rename", &fx.videos(), "--db", "tvdb", "--no-fallback"],
            "",
        )
        .await;
    assert!(out.contains("Da TVDB"), "{out}");
}

#[tokio::test]
async fn senza_chiave_spiega_come_inserirla() {
    let fx = Fixture::with_providers("cli-nokey", Vec::new());
    fx.video("Show.Name.S01E01.mkv");
    let (result, _) = fx.run(&["rename", &fx.videos()], "").await;
    let error = result.unwrap_err().to_string();
    assert!(error.contains("manca la chiave API di TMDB"), "{error}");
    assert!(error.contains("renbot keys set tmdb"), "{error}");
}

#[tokio::test]
async fn cartella_senza_video() {
    let fx = Fixture::new("cli-empty");
    let (result, out) = fx.run(&["rename", &fx.videos()], "").await;
    assert!(!result.unwrap());
    assert!(out.contains("Nessun file video trovato"), "{out}");
}

#[tokio::test]
async fn episodes_stampa_l_elenco_filtrato() {
    let fx = Fixture::new("cli-episodes");
    let (result, out) = fx.run(&["episodes", "show name"], "").await;
    assert!(result.unwrap());
    assert!(out.starts_with("Show Name (2019) · TMDB\n"), "{out}");
    assert!(out.contains("S01E01  2020-01-01  Pilota"), "{out}");
    assert!(out.contains("S02E01  2020-02-01  Ritorno"), "{out}");

    let (_, out) = fx
        .run(&["episodes", "show name", "--season", "2"], "")
        .await;
    assert!(!out.contains("Pilota"), "{out}");
    assert!(out.contains("Ritorno"), "{out}");

    let (result, out) = fx
        .run(&["episodes", "show name", "--choice", "5"], "")
        .await;
    assert!(!result.unwrap());
    assert!(out.contains("Nessuna serie trovata"), "{out}");
}

fn saved_list(fx: &Fixture) -> PathBuf {
    let series = SearchResult {
        provider: ProviderId::Tmdb,
        id: "1".to_string(),
        kind: MediaKind::Episode,
        title: "Show Name".to_string(),
        original_title: None,
        year: Some(2019),
    };
    let episode = |number, title: &str| Episode {
        season: 1,
        number,
        title: title.to_string(),
        air_date: None,
    };
    let episodes = vec![episode(1, "Pilota"), episode(2, "Secondo")];
    let mut list = RenameList::new(series, "it", episodes);
    list.add_files(["t1.mkv", "t2.mkv", "t3.mkv", "t4.mkv"].map(|name| fx.video(name)));
    list.auto_assign(&[0, 1], 1, 1);
    list.assign(
        2,
        Some(EpisodeRef {
            season: 1,
            number: 2,
        }),
    );
    list.set_included(3, false);
    let path = fx.dir.path().join("lista.json");
    list.save(&path).unwrap();
    path
}

#[tokio::test]
async fn list_show_e_apply() {
    let fx = Fixture::new("cli-list");
    let file = saved_list(&fx);
    let file = file.to_str().unwrap();

    let (result, out) = fx.run(&["list", "show", file], "").await;
    assert!(result.unwrap());
    assert!(out.contains("[Pronto]"), "{out}");
    assert!(out.contains("[Conflitto]"), "{out}");
    assert!(
        out.contains("lo stesso episodio è assegnato a più righe"),
        "{out}"
    );
    assert!(out.contains("[Escluso]"), "{out}");
    assert!(out.contains("4 righe · 1 pronte"), "{out}");
    assert!(fx.exists(&["t1.mkv"]));

    let (result, out) = fx.run(&["list", "apply", file], "").await;
    assert!(result.unwrap());
    assert!(out.contains("Rinominati 1 file"), "{out}");
    assert!(fx.exists(&RENAMED_FIRST));
    assert!(
        fx.exists(&["t2.mkv"]),
        "le righe in conflitto non sono toccate"
    );
    assert!(fx.exists(&["t4.mkv"]), "le righe escluse non sono toccate");

    let (result, _) = fx.run(&["list", "show", "inesistente.json"], "").await;
    assert!(result.is_err());
}

#[tokio::test]
async fn keys_set_verify_delete() {
    let fx = Fixture::new("cli-keys");

    let (result, out) = fx.run(&["keys", "verify", "tmdb"], "").await;
    assert!(!result.unwrap());
    assert!(out.contains("Nessuna chiave salvata"), "{out}");

    let (result, out) = fx.run(&["keys", "set", "tmdb"], "  chiave-valida\n").await;
    assert!(result.unwrap());
    assert!(out.contains("Chiave salvata"), "{out}");
    assert!(
        !out.contains("chiave-valida"),
        "la chiave non va mai stampata: {out}"
    );
    assert_eq!(fx.keys.get(ApiKey::Tmdb).unwrap().0, "chiave-valida");

    let (result, out) = fx.run(&["keys", "verify", "tmdb"], "").await;
    assert!(result.unwrap());
    assert!(out.contains("Chiave valida"), "{out}");

    fx.run(&["keys", "set", "tvdb"], "sbagliata\n")
        .await
        .0
        .unwrap();
    let (result, out) = fx.run(&["keys", "verify", "tvdb"], "").await;
    assert!(!result.unwrap());
    assert!(out.contains("non valida"), "{out}");
    assert!(!out.contains("sbagliata"), "{out}");

    let (result, _) = fx.run(&["keys", "delete", "tmdb"], "").await;
    assert!(result.unwrap());
    assert!(fx.keys.get(ApiKey::Tmdb).is_none());

    let (result, out) = fx.run(&["keys", "set", "omdb"], "\n").await;
    assert!(!result.unwrap());
    assert!(out.contains("Nessuna chiave inserita"), "{out}");
}

#[tokio::test]
async fn parse_mostra_i_campi_riconosciuti() {
    let fx = Fixture::new("cli-parse");
    let (result, out) = fx
        .run(&["parse", "Show.Name.S01E02.1080p.WEB-DL.x264-GRP.mkv"], "")
        .await;
    assert!(result.unwrap());
    for line in [
        "tipo       episodio",
        "titolo     Show Name",
        "anno       —",
        "stagione   1",
        "episodio   2",
        "qualità    1080p WEB-DL",
        "gruppo     GRP",
        "estensione mkv",
    ] {
        assert!(out.contains(line), "manca «{line}» in:\n{out}");
    }
}

#[test]
fn argomenti_non_validi_sono_rifiutati() {
    for args in [
        vec!["renbot", "rename"],
        vec!["renbot", "rename", "x", "--db", "anidb"],
        vec!["renbot", "keys", "set", "imdb"],
        vec!["renbot", "sconosciuto"],
    ] {
        assert!(Cli::try_parse_from(&args).is_err(), "{args:?}");
    }
}
