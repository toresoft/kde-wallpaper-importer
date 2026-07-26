# KDE Wallpaper Importer — Design

Data: 2026-07-26
Stato: approvato

## Obiettivo

Aggiungere al menù contestuale di Dolphin una voce che importa un'immagine
nella libreria wallpaper di KDE/XDG dell'utente, generando un pacchetto
KPackage valido e garantendo l'assenza di collisioni di nome con i wallpaper
già installati (utente e sistema).

## Ambiente di riferimento

Fedora 44, Plasma 6.7.3, KF6. Formato servicemenu verificato sui file di
sistema: `Type=Service` + `MimeType=` + `Actions=`, in
`<datadir>/kio/servicemenus/`.

Il file va installato **eseguibile (0755)**. I servicemenu di sistema sono 0644
ma appartengono a root; per quelli non di root KDE pretende il bit di
esecuzione e altrimenti rifiuta il file:

```
Access to ".../kde-wallpaper-importer.desktop" denied,
not owned by root and executable flag not set.
```

Con 0644 la voce semplicemente non compare nel menù, senza alcun messaggio
visibile all'utente.

Prerequisito di build: `sudo dnf install rust cargo`. `gcc`, `ld` e
`pkg-config` sono già presenti.

## Scelte di fondo

| Decisione | Scelta |
|---|---|
| Formato di importazione | pacchetto KPackage completo (metadata + screenshot) |
| Strategia collisioni | slug + suffisso numerico, dedup per hash del contenuto |
| Voci di menù | «Importa come sfondo» e «Importa e imposta come sfondo» |
| Idoneità immagine | filtro MIME nel `.desktop` + controllo runtime con conferma |
| Stack | Rust, crate `image`, dialoghi via `kdialog`/`notify-send` |

## 1. Architettura

Un unico binario Rust `kde-wallpaper-import`, invocato da un file servicemenu.
Nessun demone: processo one-shot che termina.

```
Dolphin ──(Exec=%F)──> kde-wallpaper-import ──> ~/.local/share/wallpapers/<Nome>/
                              │                        (pacchetto KPackage)
                              ├──> kdialog        (conferme, errori bloccanti)
                              ├──> notify-send    (esito)
                              └──> plasma-apply-wallpaperimage  (solo con --apply)
```

### Superficie CLI

La CLI è anche l'interfaccia di test: ogni comportamento è pilotabile da riga
di comando.

```
kde-wallpaper-import [OPZIONI] <FILE>...
  --apply                 imposta come sfondo l'ultimo pacchetto elaborato
  --fill-mode <MODE>      passato a plasma-apply-wallpaperimage
  --dest <DIR>            root di destinazione
                          (default: ${XDG_DATA_HOME:-$HOME/.local/share}/wallpapers)
  --no-ui                 nessun kdialog/notify-send; solo stdout/stderr ed exit code
  --force                 salta le conferme (immagini piccole)
  --min-size <WxH>        soglia di avviso (default 1024x768)
```

## 2. Componenti

Moduli piccoli, a responsabilità singola, testabili in isolamento.

| Modulo | Responsabilità | Dipende da |
|---|---|---|
| `main.rs` | parsing clap, orchestrazione del ciclo, exit code | tutti |
| `probe.rs` | apre l'immagine, restituisce `ImageInfo { width, height, format }` o errore tipizzato | `image` |
| `naming.rs` | `slug(filename)` → nome pacchetto; risoluzione collisioni con suffisso | nessuna (puro) |
| `catalog.rs` | enumera i pacchetti esistenti in tutte le `wallpapers/` XDG; mappa `hash → dir` | fs |
| `package.rs` | scrive il KPackage in modo atomico (tmp dir + rename) | `image`, `serde_json`, `sha2`, `libc` |
| `ui.rs` | trait `Ui`; impl `KdeUi` (kdialog/notify-send) e `SilentUi` (`--no-ui`) | `std::process` |
| `apply.rs` | trait `WallpaperSetter`; impl che invoca `plasma-apply-wallpaperimage` | `std::process` |

`ui.rs` e `apply.rs` sono dietro trait perché la suite di test non deve mai
aprire finestre né modificare lo sfondo reale.

### Dipendenze

```toml
[dependencies]
image      = { version = "0.25", default-features = false,
               features = ["jpeg", "png", "webp", "tiff", "bmp"] }
sha2       = "0.10"
serde_json = "1"
anyhow     = "1"
clap       = { version = "4", features = ["derive"] }
libc       = "0.2"          # renameat2(RENAME_NOREPLACE)

[dev-dependencies]
tempfile   = "3"
assert_cmd = "2"
predicates = "3"
```

Nessuna dipendenza C oltre libc.

## 3. Servicemenu

File unico `<PREFIX>/share/kio/servicemenus/kde-wallpaper-importer.desktop`,
generato a install time da `share/kio/servicemenus/kde-wallpaper-importer.desktop.in`.

```ini
[Desktop Entry]
Type=Service
MimeType=image/jpeg;image/png;image/webp;image/tiff;image/bmp;
Actions=ImportWallpaper;ImportAndApplyWallpaper;
X-KDE-Priority=TopLevel

[Desktop Action ImportWallpaper]
Icon=preferences-desktop-wallpaper
Name=Import as wallpaper
Name[it]=Importa come sfondo
Exec=@BINARY@ %F

[Desktop Action ImportAndApplyWallpaper]
Icon=preferences-desktop-wallpaper
Name=Import and set as wallpaper
Name[it]=Importa e imposta come sfondo
Exec=@BINARY@ --apply %F
```

Due dettagli non ovvi:

- `%F` (non `%u`) abilita la **selezione multipla** di file locali.
- `@BINARY@` è sostituito dall'installer con il **percorso assoluto** del
  binario. Dolphin non eredita necessariamente `~/.local/bin` nel `PATH`;
  affidarsi al nome nudo del comando produce la voce di menù che «c'è ma non
  fa nulla».

AVIF, JXL, SVG e GIF non sono nella lista MIME: la voce non compare per quei
file.

## 4. Flusso dati

Per ogni file della selezione:

1. `probe` → `ImageInfo`. Decodifica fallita → errore, file saltato.
2. Se `w × h` è sotto la soglia → `ui.confirm("Immagine piccola (640×480).
   Importare comunque?")`. Con `--force` o `--no-ui` si procede senza chiedere.
3. SHA-256 del file sorgente.
4. `catalog`: se un pacchetto già importato **nella root di destinazione** ha
   lo stesso hash → **skip**, contato come duplicato, con il nome del pacchetto
   esistente nel riepilogo. Il dedup guarda solo la destinazione; il controllo
   sui nomi (punto 5) guarda invece tutte le root XDG.
5. `naming`: primo nome libero rispetto a tutte le directory wallpaper XDG.
6. `package::write`: costruzione in tmp dir e `rename` atomico.

A fine ciclo: una sola notifica riassuntiva (`3 importati, 1 duplicato,
1 errore`); con `--apply`, `plasma-apply-wallpaperimage` sull'ultimo pacchetto
**elaborato con successo** — importato oppure riconosciuto come duplicato.

Il duplicato conta perché la voce di menù «Importa e imposta come sfondo» deve
fare ciò che promette anche su un'immagine già in libreria: il pacchetto esiste
già su disco e `catalog::duplicate_of` ne restituisce il nome, quindi non c'è
ragione di non applicarlo. Limitare `--apply` ai soli import nuovi renderebbe
quella voce un no-op silenzioso nel caso più frequente dopo il primo utilizzo.

### Struttura prodotta

```
~/.local/share/wallpapers/Tramonto_Cala_Luna/
├── metadata.json
└── contents/
    ├── screenshot.png        anteprima ridotta (max 1280px lato lungo, Lanczos3)
    └── images/
        └── 3840x2160.jpg     copia bit-per-bit dell'originale
```

```json
{
  "KPlugin": {
    "Id": "Tramonto_Cala_Luna",
    "Name": "Tramonto Cala Luna",
    "License": "Unknown"
  },
  "X-KWI": {
    "SourceSha256": "a3f9...",
    "SourcePath": "/home/utente/foto/tramonto.jpg",
    "ImportedAt": "2026-07-26T09:12:00Z"
  }
}
```

`X-KWI.SourceSha256` rende il dedup economico: `catalog` legge solo i
`metadata.json`, senza ri-hashare le immagini installate. I pacchetti di
sistema non hanno la chiave, quindi per loro vale il solo controllo sul nome.

`Authors` non viene popolato: non ha senso attribuire l'immagine all'utente che
la importa.

## 5. Naming e collisioni

### Slug

`naming::slug()` è una funzione pura senza I/O.

| Regola | Esempio |
|---|---|
| si parte dallo stem del file | `Tramonto Cala Luna.jpg` → `Tramonto Cala Luna` |
| spazi → `_`, run collassati | → `Tramonto_Cala_Luna` |
| rimossi `/`, NUL, control chars, punti iniziali | `.nascosto.png` → `nascosto` |
| trim di `-`, `_`, spazi ai bordi | `--foto--` → `foto` |
| troncamento a 60 **caratteri** (non byte) | nomi lunghi tagliati su boundary UTF-8 |
| stem vuoto → `Wallpaper`; `.` e `..` → `Wallpaper` | `.jpg` → `Wallpaper` |
| maiuscole e lettere non-ASCII preservate | `Fjörð.png` → `Fjörð` |

Separazione voluta: `_` compare solo dalla normalizzazione, `-N` solo dalle
collisioni. Dal nome si capisce sempre l'origine del suffisso.

### Ricerca del nome libero

L'insieme dei nomi occupati è l'unione delle sottodirectory di
`<root>/wallpapers` per **ogni** root XDG: `$XDG_DATA_HOME` (default
`~/.local/share`) più tutte le `$XDG_DATA_DIRS`.

Il confronto è **case-insensitive**: `foresta` e `Foresta` sono in collisione.
Sono directory distinte per il filesystem, ma nel selettore di Plasma
sarebbero due voci indistinguibili.

Includere le root di sistema è necessario, non prudenziale: un pacchetto utente
con lo stesso Id di uno di sistema **lo maschera** nella risoluzione KPackage,
facendo sparire il wallpaper originale dal selettore. Da qui `Altai` →
`Altai-2`.

Ciclo: `Nome`, `Nome-2`, `Nome-3`, … fino al primo libero. Limite 999, oltre il
quale è errore.

## 6. Scrittura atomica

Il pacchetto è costruito in `<root>/.kwi-tmp-<pid>-<n>/` e spostato a
destinazione con `renameat2(RENAME_NOREPLACE)`: o la directory finale appare
completa, o non appare affatto, e il rename fallisce se nel frattempo il nome è
stato occupato — in quel caso si passa al suffisso successivo, massimo 5
tentativi.

Su kernel o filesystem che restituiscono `EINVAL`/`ENOSYS` si degrada a
check-then-`rename`.

La tmp dir è protetta da un guard RAII che la rimuove su ogni percorso di
uscita, errori inclusi. All'avvio il programma rimuove eventuali `.kwi-tmp-*`
più vecchie di 24h lasciate da crash precedenti.

Interruzione a metà di una selezione multipla: i file già importati restano,
nessun pacchetto è a metà.

## 7. Gestione errori

`probe.rs` restituisce errori tipizzati, ognuno con messaggio utente
specifico.

| Errore | Messaggio |
|---|---|
| `Unreadable` | «Impossibile leggere *file*: permesso negato» |
| `UnsupportedFormat` | «Formato non supportato (AVIF). Serve una build con supporto dav1d.» |
| `Corrupt` | «*file* non è un'immagine valida o è danneggiato» |

`TooSmall` non è un errore: è il ramo di conferma descritto al punto 4.2.

Gli errori non interrompono il ciclo: gli altri file proseguono. Alla fine una
sola notifica riassuntiva, con urgenza critica se qualcosa è fallito. I
dialoghi bloccanti `kdialog` sono riservati alle conferme, mai agli esiti: un
batch di 20 file non deve produrre 20 popup.

Exit code:

| Codice | Significato |
|---|---|
| 0 | nessun errore: ogni file è stato importato o saltato come duplicato |
| 1 | successo parziale: almeno un file gestito senza errori e almeno uno fallito |
| 2 | nessun file gestito senza errori |
| 64 | errore d'uso (argomenti) |

«Gestito senza errori» include il duplicato saltato e l'import annullato
dall'utente al dialogo di conferma: nessuno dei due è un errore.

## 8. Test

Ogni test di integrazione gira con `XDG_DATA_HOME` puntato a una tmpdir e
`--no-ui`. Le immagini di prova sono generate al volo dal crate `image`:
nessuna fixture binaria nel repository.

**Unit** (`naming`, `package`): tabella di casi sullo slug — unicode, spazi,
punti iniziali, `..`, stem vuoto, 300 caratteri, troncamento su boundary
multi-byte; serializzazione di `metadata.json`; parsing di `--min-size`.

**Integrazione** (`assert_cmd` + `tempfile`):

- import base → struttura, `metadata.json`, `screenshot.png`, nome file
  `<W>x<H>.<ext>`, originale byte-identico
- stesso file importato due volte → un solo pacchetto, secondo run segnalato
  come duplicato
- file diverso con lo stesso nome → `-2`
- collisione con wallpaper di sistema (finto `Altai` in una root fittizia) →
  `Altai-2`
- catena `-2`/`-3` e riuso del buco lasciato da un pacchetto cancellato
- immagine sotto soglia, con e senza `--force`
- file corrotto e file illeggibile → exit code corretto, nessun pacchetto
  parziale
- selezione multipla mista (ok + duplicato + errore) → exit `1`, riepilogo
  corretto
- nessun residuo `.kwi-tmp-*` dopo qualunque test

CI GitHub Actions: `cargo test`, `cargo clippy -- -D warnings`,
`cargo fmt --check`.

## 9. Installazione e layout

```
.
├── Cargo.toml
├── Makefile
├── README.md
├── src/{main,probe,naming,catalog,package,ui,apply}.rs
├── share/kio/servicemenus/kde-wallpaper-importer.desktop.in
└── tests/{import.rs,common/mod.rs}
```

`Makefile` con `PREFIX` (default `$HOME/.local`):

| Target | Azione |
|---|---|
| `make build` | `cargo build --release` |
| `make test` | `cargo test` |
| `make install` | binario in `$PREFIX/bin`; `.desktop.in` reso sostituendo `@BINARY@` con il percorso assoluto e scritto in `$PREFIX/share/kio/servicemenus/`; `kbuildsycoca6` se presente |
| `make uninstall` | rimuove binario e servicemenu; **non** tocca i wallpaper importati |

Il README documenta il prerequisito `sudo dnf install rust cargo` e segnala la
voce preesistente `setAsWallpaperFive.desktop` presente in
`~/.local/share/kio/servicemenus/`, che imposta lo sfondo via `gsettings`
(GNOME) e quindi su Plasma non ha effetto utile. Viene segnalata, non
modificata.

## Fuori scope

- installazione di sistema con `pkexec`
- `contents/images_dark/`
- editing dei metadati del pacchetto
- ridimensionamento dell'immagine originale
- watch di cartelle
- integrazione con Gwenview o con altri file manager

## Modifiche dopo l'approvazione

**2026-07-26 — `--apply` applica anche i duplicati.** La review della Task 6 ha
mostrato che limitare `--apply` ai soli import nuovi rende la voce di menù
«Importa e imposta come sfondo» un no-op silenzioso su qualunque immagine già
in libreria: nessun errore, nessun dialogo, exit 0, sfondo invariato. Poiché il
pacchetto esiste già su disco e `catalog::duplicate_of` ne restituisce il nome,
`Outcome::Duplicate` porta ora anche il path del pacchetto esistente e alimenta
l'ultimo elaborato. Deciso dall'utente; sezioni 1 e 4 aggiornate di conseguenza.

**2026-07-26 — il servicemenu va installato eseguibile (0755).** La verifica
manuale sulla macchina reale ha mostrato che con 0644 la voce non compare in
Dolphin: KDE rifiuta i file `.desktop` non di proprietà di root che non hanno il
bit di esecuzione, registrando `not owned by root and executable flag not set` e
senza mostrare nulla all'utente. Il vincolo 0644 era stato derivato dai file in
`/usr/share/kio/servicemenus/`, che sono 0644 ma appartengono a root: la
condizione vera è «root oppure eseguibile», non «0644». Sezione «Ambiente di
riferimento» corretta.
