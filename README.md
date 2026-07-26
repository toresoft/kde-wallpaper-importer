# KDE Wallpaper Importer

Aggiunge al menù contestuale di Dolphin due voci per importare un'immagine
nella libreria wallpaper di KDE:

- **Importa come sfondo** — installa il pacchetto e basta
- **Importa e imposta come sfondo** — installa e applica subito

L'immagine diventa un pacchetto KPackage in
`~/.local/share/wallpapers/<Nome>/`, quindi compare nel selettore sfondi di
Plasma con anteprima ed è disinstallabile dalla GUI.

## Prerequisiti

Una toolchain Rust stabile (1.80 o superiore):

```bash
sudo dnf install -y rust cargo        # oppure: rustup toolchain install stable
```

Se hai installato Rust con rustup, assicurati che `~/.cargo/bin` sia nel
`PATH` prima di lanciare `make`.

A runtime servono `kdialog`, `notify-send` e `plasma-apply-wallpaperimage`,
già presenti su una Plasma standard. Il programma degrada senza crash se
mancano.

## Installazione

```bash
make install              # in ~/.local
make install PREFIX=/usr/local   # per tutti gli utenti (richiede sudo)
```

Se la voce non compare, chiudere e riaprire Dolphin.

## Disinstallazione

```bash
make uninstall
```

I wallpaper già importati non vengono toccati: si rimuovono dal selettore di
Plasma o cancellando la directory sotto `~/.local/share/wallpapers/`.

## Uso da riga di comando

```
kde-wallpaper-import [OPZIONI] <FILE>...
  --apply              imposta come sfondo l'ultimo pacchetto importato
  --fill-mode <MODE>   passato a plasma-apply-wallpaperimage
  --dest <DIR>         root di destinazione
  --no-ui              nessun dialogo né notifica
  --force              salta le conferme sulle immagini piccole
  --min-size <WxH>     soglia di avviso (default 1024x768)
```

Exit code: `0` nessun errore, `1` successo parziale, `2` nessun file gestito
senza errori, `64` errore d'uso.

## Come evita le collisioni

Il nome del pacchetto deriva dal nome del file (spazi → `_`, massimo 60
caratteri). Prima di scrivere:

1. se un pacchetto già importato ha lo **stesso contenuto** (SHA-256), il file
   viene saltato come duplicato;
2. altrimenti si cerca il primo nome libero — `foresta`, `foresta-2`, … —
   confrontando **case-insensitive** contro tutte le directory `wallpapers` di
   `$XDG_DATA_HOME` e `$XDG_DATA_DIRS`, incluse quelle di sistema.

Il punto 2 include `/usr/share/wallpapers` perché un pacchetto utente con lo
stesso Id di uno di sistema lo maschererebbe, facendo sparire l'originale dal
selettore.

La scrittura è atomica: il pacchetto viene costruito in una directory
temporanea e spostato con `renameat2(RENAME_NOREPLACE)`. Non esistono
pacchetti a metà, nemmeno interrompendo il programma.

## Formati supportati

JPEG, PNG, WebP, TIFF, BMP. AVIF e JXL sono esclusi: né Pillow né ImageMagick
li decodificano nell'installazione Fedora di riferimento, e il crate `image`
richiederebbe `libdav1d`.

## Nota

Se in `~/.local/share/kio/servicemenus/` è presente
`setAsWallpaperFive.desktop`, quella voce imposta lo sfondo via `gsettings`
(GNOME) e su Plasma non ha effetto. Questo progetto non la modifica.

## Sviluppo

```bash
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt --check
```
