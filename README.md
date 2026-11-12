# renbot

Applicazione desktop per Windows, macOS e Linux che rinomina film, serie TV e anime:
riconosce il titolo dal nome del file, cerca i dati su TheMovieDB (TMDB) o TheTVDB (TVDB)
e propone i nuovi nomi in un'anteprima. Nessun file viene toccato senza la tua conferma,
i file esistenti non vengono mai sovrascritti e l'ultima rinomina si può annullare.

Scritta in Rust con interfaccia [egui](https://github.com/emilk/egui).

## Cosa fa

- **Libreria** — trascina file o cartelle: renbot abbina ogni video a un film o a un episodio
  e mostra nome originale, nuovo nome e stato (pronto, da verificare, conflitto, errore).
  Dal pannello *Corrispondenza* puoi correggere un abbinamento cercando per titolo o per ID IMDb (`tt…`).
- **Episodi** — cerca una serie e consultane l'elenco degli episodi, filtrato per stagione.
- **Liste** — per i file senza numerazione utile (ad esempio estratti da disco): scegli la serie,
  aggiungi i file e assegna gli episodi in ordine, con episodio di partenza, più stagioni,
  correzioni riga per riga, salvataggio e caricamento della lista in JSON.
- **Impostazioni** — chiavi API, formato dei nomi, database e lingua (predefinita: italiano), tema chiaro o scuro.

## Requisiti

- [Rust](https://www.rust-lang.org/tools/install) stabile (provato con la 1.97).
- CMake e un compilatore C, richiesti dalla libreria TLS. Su Windows serve anche NASM,
  oppure la variabile d'ambiente `AWS_LC_SYS_PREBUILT_NASM=1` al momento della compilazione.
- Una chiave API di TMDB o di TVDB (vedi sotto).

## Avvio

```sh
cargo run --release -p renbot
```

La prima compilazione richiede alcuni minuti. L'eseguibile si trova poi in
`target/release/renbot` (`renbot.exe` su Windows).

## Chiavi API

renbot non contiene chiavi: ognuno usa la propria. Ne basta una; con entrambe puoi attivare
"usa l'altro database se non trovo risultati".

### Come ottenerle

- **TMDB** (film, serie e anime) — crea un account gratuito su <https://www.themoviedb.org>,
  poi dalle impostazioni del profilo apri la sezione *API* e richiedi una chiave.
  renbot accetta sia la *chiave API* (v3) sia il *token di lettura* (v4).
- **TVDB** (serie e anime) — crea un account su <https://thetvdb.com> e genera una chiave
  API v4 dalla tua area personale. TheTVDB prevede condizioni d'uso proprie: leggile sul sito.
- **OMDb** (facoltativa) — richiedila su <https://www.omdbapi.com>. Per ora renbot la salva
  e la verifica soltanto; le ricerche per ID IMDb passano da TMDB.

Le procedure dei siti possono cambiare: fai riferimento alle loro pagine di aiuto.

### Come inserirle

1. Apri **Impostazioni** dalla barra laterale (al primo avvio un avviso nella Libreria ti porta lì).
2. Incolla la chiave nel campo del database. Il testo è mascherato; *Mostra* lo rende visibile.
3. Premi **Verifica chiave**: renbot esegue una chiamata reale e mostra l'esito.
4. Premi **Salva**. *Cancella* rimuove la chiave salvata.

Le chiavi sono salvate nel portachiavi del sistema operativo (Portachiavi di macOS,
Credential Manager di Windows, Secret Service su Linux). Se il portachiavi non è disponibile
finiscono in `keys.json` nella cartella di configurazione di renbot, leggibile solo dal tuo utente
dove il sistema lo consente. Le chiavi non compaiono mai nei log né nella cache.

In alternativa puoi definire le variabili d'ambiente `TMDB_API_KEY` e `TVDB_API_KEY`:
valgono quando non c'è una chiave salvata.

## Formato dei nomi

Nel formato la barra `/` separa le cartelle. Il nuovo percorso parte dalla cartella in cui si trova il file.

| Token | Significato |
|---|---|
| `{n}` | titolo del film o della serie |
| `{y}` | anno |
| `{s}` | stagione |
| `{e}` | episodio |
| `{s00e00}` | stagione ed episodio, es. `S01E02` |
| `{t}` | titolo dell'episodio |
| `{q}` | qualità ricavata dal nome originale, es. `1080p WEB-DL` |
| `{g}` | gruppo di release |
| `{ext}` | estensione |

Predefiniti:

- film: `{n} ({y})/{n} ({y}).{ext}`
- serie: `{n}/Season {s}/{n} - {s00e00} - {t}.{ext}`

I caratteri non ammessi nei nomi di file (`< > : " / \ | ? *`) vengono rimossi su tutti i sistemi,
così la libreria resta leggibile anche da dischi di rete. Un token senza valore non lascia
parentesi o trattini vuoti.

## Sicurezza delle rinomine

- L'anteprima viene sempre prima: si rinominano solo le righe spuntate e pronte, dopo una conferma.
- Se la destinazione esiste già il file viene saltato e segnalato come conflitto.
- Tra dischi diversi il file viene copiato e poi cancellato dall'origine.
- **Annulla ultima rinomina** riporta i file ai nomi originali e rimuove le cartelle create, se vuote.

## Dove sono i dati

renbot usa le cartelle standard del sistema operativo per configurazione (`settings.json`, `keys.json`),
cache delle risposte (valida 24 ore) e dati (`undo.json`, `renbot.log`). Il percorso del file
di registro è mostrato in fondo alle Impostazioni. Il registro viene riscritto a ogni avvio.

## Sviluppo

```sh
cargo test                                   # test, senza rete e senza portachiavi reale
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```

| Percorso | Contenuto |
|---|---|
| `crates/renbot-core` | libreria: `parser`, `providers`, `matcher`, `renamer`, `cache`, `config`, `scanner` |
| `crates/renbot-app` | interfaccia grafica; i testi sono tutti in `src/strings.rs` |
| `docs/filebot-analysis.md` | analisi iniziale e scelte di progetto |
| `.github/workflows/ci.yml` | build, test, clippy e fmt su Windows, macOS e Linux |

I test dei provider usano risposte HTTP simulate; quelli di rinomina lavorano solo su cartelle temporanee.

## Limiti noti

- AniDB, ordinamento assoluto degli episodi anime, sottotitoli e artwork non sono supportati.
  Per gli anime numerati in modo assoluto usa una Lista.
- TVDB viene interrogato solo per serie e anime; i film arrivano da TMDB.
- Di un file con due episodi (`S01E01E02`) viene riconosciuto solo il primo.
- Nelle Liste il riordino si fa con i pulsanti su e giù.

## Licenza

renbot è software libero, distribuito con licenza [GNU General Public License versione 3](LICENSE).
Puoi usarlo, studiarlo, modificarlo e ridistribuirlo; chi distribuisce una versione modificata
deve renderne disponibile il codice sorgente con la stessa licenza. Il programma è fornito senza alcuna garanzia.

## Riconoscimenti

renbot usa le API di TMDB e di TheTVDB ma non è approvato né certificato da questi servizi.
L'idea di prodotto si ispira ai flussi d'uso di altri programmi di rinomina; codice, testi,
icone e grafica sono originali. Icone: [Phosphor](https://phosphoricons.com).
