# Analisi di FileBot — Milestone 0 di renbot

Data: 10 novembre 2026 · Stato: **approvato dall'utente il 10/11/2026**. Decisioni: righe 17–22 incluse nell'MVP; sezione Episodi separata; token con i nomi previsti dai requisiti.

## 0. Metodo e limiti della ricerca

Consultazione in sola lettura del sito www.filebot.net. Nessun download, installazione o esecuzione di FileBot. Tutto il testo che segue è una sintesi con parole mie.

Pagine lette:

| URL | Come è stato letto | Contenuto usato |
|---|---|---|
| https://www.filebot.net/ | estratto dalla ricerca (lettura diretta rifiutata) | presentazione, flusso in tre passi, elenco funzionalità, piattaforme |
| https://www.filebot.net/naming.html | lettura diretta + estratto dalla ricerca | sintassi del formato, elenco variabili e funzioni |
| https://www.filebot.net/cli.html | estratto dalla ricerca | opzioni: database, ordine episodi, azioni, conflitti, revert |
| https://www.filebot.net/purchase.html | estratto dalla ricerca | modello di licenza |
| https://www.filebot.net/script.html | estratto dalla ricerca | automazione e script disponibili |
| https://www.filebot.net/forums/viewtopic.php?t=7 | estratto dalla ricerca (lettura diretta fallita) | manuale/FAQ: uso della GUI, scorciatoie, storico |
| https://www.filebot.net/forums/viewtopic.php?t=12553 | estratto dalla ricerca | rinomina lineare |
| https://www.filebot.net/forums/viewtopic.php?t=13578 | estratto dalla ricerca | note di rilascio 5.x |
| https://www.filebot.net/forums/viewtopic.php?t=4116 | estratto dalla ricerca | schemi di nomi per media server |
| https://www.filebot.net/forums/viewtopic.php?t=12796 | estratto dalla ricerca | correzione manuale degli abbinamenti |

"Estratto dalla ricerca" = testo della pagina restituito da una ricerca web limitata al dominio filebot.net; può essere parziale. Nessun altro sito è stato consultato.

**Limiti da tenere presenti:**

1. **Schermate.** Gli strumenti di lettura restituiscono solo testo, quindi le immagini sul sito non erano visibili. La sezione 2 è nata dal testo della documentazione; in seguito l'utente ha fornito due schermate (sezioni di rinomina e di elenco episodi, entrambe a liste vuote), descritte al punto 2.1. Le schermate non sono incluse nel repository. Restano non verificati visivamente: liste popolate, colori degli abbinamenti dubbi, editor del formato, storico, menu contestuali.
2. La lettura diretta della home e della pagina FAQ del forum non è riuscita; il loro contenuto è stato letto tramite gli estratti testuali delle stesse pagine restituiti dalla ricerca limitata al dominio filebot.net. Gli estratti del forum possono essere parziali.
3. Dove un punto è una mia deduzione e non un fatto letto, è marcato con *(deduzione)*.

---

## 1. Caratteristiche salienti

**Nucleo: rinomina di film, serie e anime**
- Abbinamento automatico dei file ai dati di un database online, poi rinomina e riorganizzazione in cartelle.
- Fonti: TheMovieDB (film e serie), TheTVDB, AniDB; OMDb per i film; altre fonti minori citate nelle note di rilascio.
- Due modalità di abbinamento: una "prudente" e una "opportunistica" che accetta corrispondenze meno certe. La GUI usa di default quella opportunistica, la riga di comando quella prudente.
- Ricerca forzata per nome o per ID numerico quando il riconoscimento automatico sbaglia.
- Diversi ordinamenti degli episodi (messa in onda, DVD, assoluto e altri).
- Lingua preferita per titoli e nomi degli episodi.
- Riconoscimento degli episodi doppi quando il nome file li indica in modo esplicito.

**Formato dei nomi**
- Segnaposto tra graffe per nome, anno, stagione, episodio, titolo ecc., combinabili con testo libero e separatori di percorso: lo stesso formato decide sia il nome sia la cartella di destinazione.
- Oltre un centinaio di variabili: metadati (regista, generi, collezione, ID dei database), dati tecnici letti dal file (risoluzione, codec, canali audio, HDR), gruppo di release, sorgente, lingua dei sottotitoli.
- Le graffe contengono vere espressioni di un linguaggio di scripting, con funzioni su stringhe (maiuscole, sostituzioni, riempimento con zeri, traslitterazione).
- Schemi pronti per i principali media server e preset salvabili.

**Sicurezza delle operazioni**
- Anteprima affiancata nomi originali / nuovi nomi prima di rinominare.
- Azioni alternative allo spostamento: copia, collegamento simbolico, hard link, prova a vuoto.
- Politiche per i conflitti: salta, sostituisci, automatico, aggiungi indice, interrompi.
- Storico interno delle rinomine con ripristino.

**Funzioni accessorie**
- Sottotitoli: ricerca (anche per hash del file), anteprima, conversione di formato e codifica.
- Artwork e file di metadati per i media server.
- Musica (tag o impronta acustica), foto (dati Exif), rinomina generica tramite pattern o prompt AI.
- Checksum: creazione e verifica (SFV, MD5, SHA), confronto di alberi di cartelle.
- Estrazione di archivi e immagini ISO.
- Riga di comando e motore di script per flussi automatici non presidiati.

**Distribuzione**
- Installer nativi per Windows, macOS e le principali distribuzioni Linux, pacchetti per NAS e immagini Docker.
- Software commerciale: prova gratuita, poi licenza per utente (annuale o a vita) valida su tutte le piattaforme.

---

## 2. Organizzazione dell'interfaccia

*(Ricostruita dal testo della documentazione, vedi limiti al punto 0.)*

**Flusso principale, in tre passi**
1. Si trascinano i file nell'elenco di sinistra (nomi originali).
2. Sull'elenco di destra (nuovi nomi) si sceglie la fonte dati da un menu contestuale; il programma abbina e popola l'elenco.
3. Si preme il pulsante di rinomina.

**Struttura**
- Applicazione divisa in sezioni per attività: rinomina, elenco episodi, sottotitoli, verifica checksum e altre.
- La sezione di rinomina è costruita su **due liste affiancate** allineate riga per riga, non su un'unica tabella con colonne (confermato dalle schermate, punto 2.1).
- Tra le due liste: pulsante di abbinamento, pulsanti per spostare su/giù gli elementi selezionati, pulsante di rinomina.

**Azioni principali**
- *Abbina*: riallinea automaticamente file e dati.
- *Riallineamento manuale*: trascinamento o frecce per spostare un elemento di una sola delle due liste.
- *Correzione di un abbinamento*: doppio clic sulla riga, poi voce di modifica; oppure clic con tasto modificatore sulla fonte dati per forzare l'inserimento manuale del titolo.
- *Esclusione*: tasto Canc rimuove la riga intera; con Maiusc rimuove solo l'elemento di una delle due liste.
- *Modifica manuale del nome proposto*: tasto dedicato, con modifica "a colonne" su più righe che condividono un prefisso.
- *Editor del formato*: finestra separata, raggiungibile dal doppio clic su un elemento dei nuovi nomi.
- *Opzioni di rinomina*: azione (sposta/copia/link), gestione dell'estensione, conflitti.
- *Storico*: finestra con le operazioni passate e voce di ripristino dal menu contestuale.
- *Dettagli dell'abbinamento*: mostrabili/nascondibili con la barra spaziatrice; tooltip con i dettagli del film.

**Segnalazioni visive**
- Gli abbinamenti dubbi sono evidenziati con gradazioni di rosso proporzionali al grado di anomalia; il rosso non blocca l'operazione, invita a ricontrollare.
- Avvisi per estensione mancante.

**Rinomina "lineare"**
- Sezione elenco episodi: si scarica la lista di una serie, poi la si trascina o incolla nella lista dei nuovi nomi; i file vengono abbinati per posizione, senza analisi dei nomi. È il rimedio documentato per i file estratti da disco, privi di numerazione utile.

### 2.1 Verifica sulle schermate fornite dall'utente

Due schermate, a liste vuote: sono le due funzioni che l'utente vuole in renbot.

**Elementi comuni**
- Barra laterale sinistra con sei sezioni, ciascuna con icona grande e nome sotto; la sezione attiva è un riquadro pieno colorato. Le prime due sono rinomina ed elenco episodi; le altre quattro riguardano sottotitoli, checksum, filtro e liste generiche.
- Titolo della sezione grande e centrato in alto. Nessuna barra di stato, nessuna barra dei menu interna.
- Icone illustrate e multicolore, di stile datato; molte senza etichetta.

**Sezione di rinomina**
- Due riquadri di pari larghezza: file originali a sinistra, nuovi nomi a destra, ciascuno con il titolo sopra.
- Tra i due, una colonna stretta con due soli pulsanti grandi, icona sopra e testo sotto: abbina e rinomina. Sono le uniche azioni con peso visivo.
- Sotto ogni riquadro una fila di pulsanti piccoli. A sinistra: sposta giù, sposta su, rimuovi, carica file (unico con etichetta), aggiorna, più due icone senza testo di cui non si intuisce la funzione. A destra: sposta giù, sposta su, allinea, scarica dati (con etichetta), incolla, opzioni.
- **Stato vuoto muto**: due riquadri bianchi senza alcun invito a trascinare i file né indicazione del primo passo.
- La scelta della fonte dati non è visibile: sta dietro il pulsante di scaricamento dati o il menu contestuale.

**Sezione elenco episodi**
- Una riga di controlli in alto, centrata: campo di ricerca con selettore della fonte dati incorporato a sinistra (solo icona), selettore della stagione ("tutte" di default), menu dell'ordinamento, menu della lingua con bandierina, pulsante di ricerca.
- Sotto, un'area risultati con una scheda dedicata alle ricerche passate (a vuoto è l'unica visibile; che ogni ricerca apra una scheda propria è una *deduzione*) e una tabella a tre colonne: serie, numero di episodi, durata.
- Anche qui nessun testo guida a tabella vuota.

**Conseguenze per renbot**
- Confermata la scelta di una tabella unica al posto delle due liste e di uno stato vuoto parlante.
- Confermata la regola "ogni pulsante ha icona *e* etichetta, o almeno un tooltip": qui 11 pulsanti su 13 sotto le liste sono solo icona.
- La riga di controlli della sezione episodi (fonte, stagione, lingua, cerca) è compatta ed efficace: la riprendo, con disegno diverso, in testa alla sezione Liste (punto 5).
- La lingua è un controllo di primo piano nel flusso episodi: rafforza la proposta della riga 18.
- L'ordinamento degli episodi resta fuori dall'MVP (riga 25).

---

## 3. Usabilità: punti di forza e punti deboli

**Punti di forza**
- Flusso minimo: trascina, scegli la fonte, rinomina. Nessuna configurazione obbligatoria per il caso semplice.
- L'anteprima affiancata rende evidente cosa succederà prima di toccare il disco.
- Rete di sicurezza reale: storico con ripristino e modalità di prova.
- Via d'uscita quando l'automatismo fallisce: abbinamento per posizione e riallineamento manuale.
- Nelle versioni recenti il programma suggerisce l'alternativa giusta quando un tentativo fallisce (altra modalità, selezione manuale, altro ordinamento).
- Un unico formato governa nome e cartelle.

**Punti deboli**
- **Scopribilità bassa.** Funzioni essenziali stanno dietro tasto destro, doppio clic o clic con Maiusc: scelta della fonte, correzione del match, editor del formato. Senza leggere la FAQ non si trovano.
- **Formato potente ma ostico.** Essendo un linguaggio di programmazione, gli esempi condivisi dagli utenti arrivano a centinaia di caratteri; la soglia tra "cambio un separatore" e "scrivo codice" è bassa.
- **Significato dei colori non autoesplicativo.** Le gradazioni di rosso richiedono una voce di FAQ per essere interpretate.
- **Due liste indipendenti** possono disallinearsi; l'eliminazione di una sola "cella" è potente ma facile da sbagliare. *(deduzione)*
- **Rinomina lineare indiretta**: richiede di passare da un'altra sezione e trascinare i dati tra pannelli; assegnare un episodio di partenza diverso dal primo non è un controllo esplicito.
- **Trappole nelle opzioni**: la FAQ documenta il caso di estensioni perse per un'opzione impostata male.
- **Comportamento diverso tra GUI e riga di comando** sul rigore dell'abbinamento.
- **Documentazione dispersa**: manuale e FAQ vivono in discussioni del forum.
- **Ampiezza funzionale** (musica, foto, checksum, archivi) che appesantisce uno strumento nato per rinominare video. *(valutazione mia)*

**Cosa ne ricavo per renbot**
- Ogni azione importante deve avere un pulsante visibile con icona ed etichetta; menu contestuali solo come scorciatoia.
- Una sola tabella con una riga per file (stato, originale, nuovo nome, match): niente liste che si disallineano.
- Stati espliciti con icona e testo, non solo colore.
- Formato a token semplici, con anteprima dal vivo.
- La rinomina per posizione diventa una funzione di primo livello (le "Liste"), con offset esplicito.

---

## 4. Classificazione delle funzionalità

Legenda: **R** = già nei requisiti di renbot · **MVP?** = propongo di aggiungerla all'MVP (entra solo se approvi) · **Dopo** = da rimandare

| # | Funzionalità di FileBot | Classe | Note |
|---|---|---|---|
| 1 | Trascinamento di file e cartelle, scansione ricorsiva | R | req. 1 |
| 2 | Riconoscimento di titolo, anno, stagione/episodio dal nome | R | req. 2 |
| 3 | Scelta della fonte dati TMDB / TVDB | R | req. 3, con fallback automatico |
| 4 | OMDb per i film | R | provider opzionale |
| 5 | Ricerca per ID | R | ID IMDb tramite TMDB |
| 6 | AniDB | Dopo | fuori scopo dichiarato |
| 7 | Abbinamento automatico con punteggio | R | req. 4 |
| 8 | Correzione manuale del match | R | req. 4 |
| 9 | Rinomina per posizione con elenco episodi | R | req. 5 (Liste) |
| 10 | Riallineamento manuale, esclusione di righe | R | req. 5 |
| 11 | Formato a segnaposto per nome e cartelle | R | req. 6 |
| 12 | Anteprima prima della rinomina | R | req. 7 |
| 13 | Rilevamento conflitti senza sovrascrittura | R | req. 8 (politica unica: salta) |
| 14 | Annullamento dell'ultima operazione | R | req. 8 |
| 15 | Cache delle risposte | R | req. 9 |
| 16 | Tema scuro | R | req. 11 |
| 17 | **Evidenziazione dei match dubbi** | MVP? | stato "da verificare" (icona + testo) sotto una soglia di punteggio; il punteggio esiste già |
| 18 | **Lingua dei metadati** | MVP? | **consigliata**: scelta della lingua di titoli/episodi (es. italiano con ripiego sull'inglese), predefinita nelle Impostazioni e modificabile nella barra di ricerca delle Liste; entrambi i provider la supportano |
| 19 | **Preset di formato** | MVP? | menu con 2–3 schemi pronti oltre ai due predefiniti; nessuna nuova logica |
| 20 | **Token per qualità e gruppo** | MVP? | il parser li estrae già (req. 2) ma il formato non può usarli; es. `{q}` e `{g}` |
| 21 | **Tasto Canc per escludere la riga** | MVP? | scorciatoia per l'esclusione già prevista |
| 22 | Suggerimenti quando il match fallisce | MVP? | messaggio non bloccante: "prova l'altro database" / "cerca manualmente"; in parte coperto dal fallback |
| 22b | **Consultazione dell'elenco episodi** in una sezione propria | **Approvata** | richiesta dall'utente il 10/11/2026; vedi "Episodi" al punto 5 |
| 23 | Modifica manuale del nuovo nome | Dopo | utile, ma aggiunge stati alla tabella |
| 24 | Episodi doppi (un file, due episodi) | Dopo | richiede modifiche a parser, formato e Liste |
| 25 | Ordinamenti alternativi (DVD, assoluto) | Dopo | assoluto fuori scopo dichiarato |
| 26 | Azioni copia / link simbolico / hard link | Dopo | l'MVP sposta soltanto |
| 27 | Politiche di conflitto alternative | Dopo | |
| 28 | Storico completo con ripristino selettivo | Dopo | l'MVP annulla solo l'ultima operazione |
| 29 | Formato con espressioni e funzioni | Dopo | eventualmente pochi modificatori semplici |
| 30 | Variabili tecniche lette dal file (codec, risoluzione reale) | Dopo | richiede analisi del contenuto |
| 31 | Rinomina di cartelle | Dopo | |
| 32 | Apri file / mostra nella cartella | Dopo | richiederebbe un crate non in elenco |
| 33 | Sottotitoli | Dopo | fuori scopo dichiarato |
| 34 | Artwork e file di metadati | Dopo | fuori scopo dichiarato |
| 35 | Musica, foto, rinomina generica, prompt AI | Dopo | |
| 36 | Checksum e confronto cartelle | Dopo | |
| 37 | Estrazione archivi | Dopo | |
| 38 | Riga di comando e scripting | Dopo | la libreria separata lo rende possibile in futuro |
| 39 | Installer nativi e pacchetti | Dopo | la CI produce solo build e test |

**Un punto su cui serve una tua decisione**

- **Nomi dei token.** I token dei requisiti (`{n}` `{y}` `{s}` `{e}` `{s00e00}` `{t}`) coincidono con i nomi usati da FileBot. Sono nomi funzionali e comodi per chi proviene da lì, ma dato il vincolo "design originale" segnalo la coincidenza: posso tenerli come da requisito oppure usare nomi italiani/estesi (es. `{titolo}` `{anno}`). Non sono un legale: la scelta è tua.

---

## 5. Proposta di layout per renbot

Design originale: una finestra, barra laterale fissa, una tabella unica al centro, pannello di dettaglio a destra solo quando serve. Icone phosphor (nomi indicativi tra parentesi).

### Struttura generale

```
┌────────────┬───────────────────────────────────────────────────────────┐
│  renbot    │  Barra azioni della sezione                               │
│            ├───────────────────────────────────────────┬───────────────┤
│ ▸ Libreria │                                           │               │
│   Episodi  │   Area centrale: tabella di anteprima     │  Pannello     │
│   Liste    │                                           │  dettaglio    │
│            │                                           │  (a scomparsa)│
│            │                                           │               │
│ ─────────  ├───────────────────────────────────────────┴───────────────┤
│ Impostaz.  │  Barra di stato: messaggi · avanzamento · azione primaria │
└────────────┴───────────────────────────────────────────────────────────┘
```

- Barra laterale: Libreria (`folders`), Episodi (`television-simple`), Liste (`list-numbers`), in basso Impostazioni (`gear`). La voce Episodi è stata richiesta dall'utente dopo la prima stesura ed estende il requisito 11, che ne prevedeva tre.
- Barra di stato sempre visibile: messaggio non bloccante a sinistra, barra di avanzamento al centro, pulsante primario a destra.

### Libreria (rinomina con match automatico)

```
[+ File] [+ Cartella]   Database: (TMDB | TVDB)   ☑ usa l'altro se non trovo   [Svuota]
────────────────────────────────────────────────────────────────────────────────
 ☑  Stato          Nome originale              →  Nuovo nome             Match
 ☑  ✓ Pronto       Show.Name.S01E02.1080p…        Show Name/Season 1/…   Show Name (2019)
 ☑  ⚠ Da verif.    show_nam_103.mkv               Show Name/Season 1/…   Show Name (2019) 62%
 ☐  ⛔ Conflitto    Film.2010.mkv                  Film (2010)/Film…      Film (2010)
 ☑  ⏳ In attesa    Altro.Film.mkv                 —                      —
────────────────────────────────────────────────────────────────────────────────
 4 file · 2 pronti · 1 da verificare · 1 conflitto        [↶ Annulla ultima]  [Rinomina 2 file]
```

- **Stato vuoto**: grande area tratteggiata con icona (`tray-arrow-down`), testo "Trascina qui file o cartelle" e i due pulsanti di selezione.
- **Stati** con icona + etichetta: Pronto (`check-circle`), Da verificare (`warning`), Conflitto (`prohibit`), Errore (`x-circle`), In attesa (`hourglass`), Rinominato (`check-fat`).
- **Database** come controllo segmentato visibile, non in un menu contestuale.
- **Clic su una riga** apre il pannello di destra "Corrispondenza": dati estratti dal nome (titolo, anno, SxxExx, qualità, gruppo), campo di ricerca, elenco dei candidati ordinati per punteggio, pulsante "Usa questo". La scelta si applica a tutti i file della stessa serie.
- **Pulsante primario** con conteggio ("Rinomina N file"); apre una conferma riepilogativa con il numero di file e la cartella di destinazione. Le righe con conflitto o errore non vengono mai incluse.
- **Annulla ultima** (`arrow-counter-clockwise`) attivo solo se esiste un log di annullamento.

### Episodi (consultazione dell'elenco episodi) — approvata dall'utente

Schermata di sola consultazione: non tocca alcun file.

```
(TMDB | TVDB)  [ cerca serie…              ]  Lingua: (Italiano ▾)*   [🔍 Cerca]
────────────────────────────────────────────────────────────────────────────────
 Risultati                    │  Show Name (2019) · TMDB · 3 stagioni · 30 episodi
 ▸ Show Name (2019)   30 ep.  │  Stagione: (Tutte ▾)                [Crea lista →]
   Show Name (1987)   12 ep.  │ ──────────────────────────────────────────────
   Show Name: Extra    6 ep.  │  S  Ep  Titolo                    Data
                              │  1   1  Pilot                     2019-09-12
                              │  1   2  Titolo episodio           2019-09-19
                              │  1   3  Titolo episodio           2019-09-26
                              │  2   1  Titolo episodio           2020-10-01
────────────────────────────────────────────────────────────────────────────────
 30 episodi · dati in cache (aggiornati 2 ore fa)                   [↻ Aggiorna]
```

- **Sinistra**: serie trovate, ordinate per punteggio, con anno e numero di episodi. Clic per caricare l'elenco.
- **Destra**: tabella degli episodi della serie scelta (stagione, numero, titolo, data di prima messa in onda), filtrabile per stagione; "Tutte" di default.
- **Crea lista →** (`list-plus`) apre la sezione Liste con la serie già selezionata: è il collegamento tra le due funzioni, al posto del trascinamento tra pannelli.
- **Aggiorna** ignora la cache di 24 h per la serie mostrata.
- Stato vuoto: icona e testo "Cerca una serie per vederne gli episodi".
- Riusa ricerca, provider e cache già previsti: nessun nuovo crate.
- Fuori dall'MVP: ordinamenti alternativi, storico delle ricerche, colonna durata, copia/esportazione dell'elenco.
- \* Lingua solo se approvi la riga 18.

### Liste (rinomina per posizione, più episodi)

```
(TMDB | TVDB)  [ cerca serie…          ]  Lingua: (Italiano ▾)*  [🔍 Cerca]   [Apri…] [Salva…]
Serie scelta: Show Name (2019) · 3 stagioni · 30 episodi                      [Cambia]
Stagione: (1 ▾)   Inizia dall'episodio: [ 1 ]   [Assegna in ordine]   [+ File]
────────────────────────────────────────────────────────────────────────────────
 ⠿  ☑  File                 Stagione  Episodio                    Nuovo nome
 ⠿  ☑  disc1_t01.mkv        (1 ▾)     (1 · Pilot ▾)               Show Name - S01E01 - Pilot.mkv
 ⠿  ☑  disc1_t02.mkv        (1 ▾)     (2 · Titolo ▾)              Show Name - S01E02 - …
 ⠿  ☐  disc1_t03.mkv        (1 ▾)     (—)                         escluso
 ⠿  ☑  disc2_t01.mkv        (2 ▾)     (1 · Titolo ▾)              Show Name - S02E01 - …
────────────────────────────────────────────────────────────────────────────────
 4 righe · 3 confermate                                         [Applica lista (3)]
```

- La serie si sceglie qui (ricerca nella barra in alto) oppure arriva già selezionata da "Crea lista →" nella sezione Episodi. Poi si aggiungono i file e si assegnano gli episodi; non serve trascinare dati tra pannelli.
- \* Il menu della lingua compare solo se approvi la riga 18.
- Maniglia (`dots-six-vertical`) per riordinare trascinando; frecce su/giù come alternativa da tastiera.
- Stagione ed episodio sono menu a tendina per riga, popolati dal provider; più stagioni convivono nella stessa lista.
- "Assegna in ordine" riempie le righe selezionate a partire dall'offset indicato.
- Un episodio assegnato a due righe viene segnalato come conflitto prima dell'applicazione.
- Salva/Apri usano il file picker nativo (JSON).

### Impostazioni

Sezioni impilate in un'unica pagina scorrevole:

1. **Chiavi API** — una scheda per provider (TMDB, TVDB, OMDb opzionale): campo mascherato, `eye`/`eye-slash`, [Verifica chiave] con esito in linea (`check-circle` verde / `x-circle` rosso + messaggio), [Cancella]. Indicazione di dove è salvata la chiave (portachiavi di sistema / file / variabile d'ambiente).
2. **Formato dei nomi** — un campo per film e uno per serie, elenco dei token cliccabili, anteprima dal vivo su un esempio, pulsante "Ripristina predefinito".
3. **Aspetto** — tema Chiaro / Scuro / Sistema.

Al primo avvio senza chiavi: banner in cima alla Libreria ("Per cercare i titoli serve una chiave API") con pulsante "Apri Impostazioni".

### Stile

- Un solo colore d'accento per l'azione primaria; verde/ambra/rosso riservati agli stati.
- Griglia di spaziatura a multipli di 4 px, angoli arrotondati uniformi, righe di tabella alte e alternate.
- Testo dei nomi file a larghezza variabile con ellissi centrale e tooltip con il percorso completo.
- Nessuna finestra modale tranne la conferma di rinomina e il file picker.

---

## 6. Cosa mi serve da te per procedere

1. Approvazione (o correzioni) di questo documento.
2. Per ciascuna voce **MVP?** (righe 17–22): sì / no.
3. Decisione sui nomi dei token.

Tutti i punti sono stati decisi (vedi stato in testa al documento).
