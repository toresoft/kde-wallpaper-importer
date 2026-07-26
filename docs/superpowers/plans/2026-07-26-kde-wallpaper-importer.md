# KDE Wallpaper Importer — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Un binario Rust invocato dal menù contestuale di Dolphin che importa immagini come pacchetti wallpaper KPackage in `~/.local/share/wallpapers`, senza mai collidere con i nomi già installati.

**Architecture:** Crate Rust con `src/lib.rs` (moduli puri e testabili) e un bin sottile `kde-wallpaper-import`. Ogni file selezionato attraversa la pipeline probe → hash → dedup → naming → scrittura atomica. UI (`kdialog`/`notify-send`) e applicazione dello sfondo (`plasma-apply-wallpaperimage`) sono dietro trait, così i test non aprono finestre né toccano lo sfondo reale.

**Tech Stack:** Rust 2021, `image` 0.25, `sha2`, `serde_json`, `anyhow`, `clap` 4, `libc`. Test con `tempfile`, `assert_cmd`, `predicates`. Installazione via `Makefile`.

**Spec di riferimento:** `docs/superpowers/specs/2026-07-26-kde-wallpaper-importer-design.md`

## Global Constraints

- Target: Fedora 44, Plasma 6.7.3, KF6. Servicemenu in `<PREFIX>/share/kio/servicemenus/`, `Type=Service`, permessi 0644.
- Dipendenze runtime esterne: solo `kdialog`, `notify-send`, `plasma-apply-wallpaperimage`, tutte opzionali e invocate come processi. Nessuna dipendenza C oltre `libc`.
- Crate consentiti: `image`, `sha2`, `serde_json`, `anyhow`, `clap`, `libc`; dev: `tempfile`, `assert_cmd`, `predicates`. Non aggiungerne altri senza aggiornare la spec.
- `image` va usato con `default-features = false` e le sole feature `jpeg`, `png`, `webp`, `tiff`, `bmp`.
- MIME esposti dal servicemenu: `image/jpeg;image/png;image/webp;image/tiff;image/bmp;` — niente AVIF, JXL, SVG, GIF.
- Costanti fissate dalla spec: slug max **60 caratteri**, fallback `Wallpaper`, suffisso collisioni max **999**, screenshot max **1280 px** lato lungo con Lanczos3, soglia default **1024x768**, sweep tmp oltre **24h**, prefisso tmp `.kwi-tmp-`.
- Exit code: `0` nessun errore, `1` successo parziale, `2` nessun file gestito senza errori, `64` errore d'uso. Duplicato saltato e conferma annullata **non** sono errori.
- Confronto nomi **case-insensitive**; il controllo collisioni copre `$XDG_DATA_HOME` e tutte le `$XDG_DATA_DIRS`, il dedup per hash solo la root di destinazione.
- Ogni funzione che legge l'ambiente deve avere una controparte pura che riceve i valori come parametri: i test non mutano variabili d'ambiente di processo.
- `cargo fmt` e `cargo clippy --all-targets -- -D warnings` devono restare puliti a ogni commit.
- Messaggi utente in italiano; identificatori, commenti di codice e messaggi di commit in italiano, coerenti con la spec.
- Non aggiungere trailer `Co-Authored-By` ai commit.
- Prima di qualunque comando `cargo`, eseguire `export PATH="$HOME/.cargo/bin:$PATH"`: la toolchain è installata via rustup e non è nel `PATH` di default.

## Prerequisiti

Rust è già installato via **rustup** (`rustc` 1.96.1, con `clippy` e `rustfmt`),
ma `~/.cargo/bin` **non è nel `PATH`** di una shell non interattiva. Ogni
comando `cargo` di questo piano va eseguito dopo:

```bash
export PATH="$HOME/.cargo/bin:$PATH"
```

Verifica: `cargo --version` deve stampare `cargo 1.96.1` o superiore.

`gcc`, `ld` e `pkg-config` sono presenti, non serve altro. Niente `dnf`.

## Struttura dei file

| File | Responsabilità |
|---|---|
| `Cargo.toml` | manifest, dipendenze, target lib + bin |
| `src/lib.rs` | dichiara i moduli pubblici, nient'altro |
| `src/main.rs` | bin sottile: chiama `run::main()` ed esce col codice restituito |
| `src/naming.rs` | slug e risoluzione collisioni. Funzioni pure, zero I/O |
| `src/probe.rs` | validazione immagine e dimensioni, errori tipizzati |
| `src/catalog.rs` | root XDG, nomi occupati, mappa hash → pacchetto |
| `src/package.rs` | hash, timestamp ISO, screenshot, scrittura atomica del KPackage |
| `src/ui.rs` | trait `Ui`, impl `KdeUi` / `SilentUi` / fake per test |
| `src/apply.rs` | trait `WallpaperSetter`, impl `PlasmaSetter` |
| `src/cli.rs` | struct clap `Cli` e parsing di `--min-size` |
| `src/run.rs` | orchestrazione, riepilogo, exit code |
| `tests/common/mod.rs` | helper: tmpdir, generazione immagini, invocazione del binario |
| `tests/import.rs` | test di integrazione end-to-end |
| `share/kio/servicemenus/kde-wallpaper-importer.desktop.in` | template servicemenu con `@BINARY@` |
| `Makefile` | build, test, install, uninstall |
| `README.md` | prerequisiti, installazione, uso |
| `.github/workflows/ci.yml` | fmt, clippy, test |

Rispetto alla spec sono stati aggiunti `lib.rs`, `cli.rs` e `run.rs`: la spec metteva l'orchestrazione in `main.rs`, ma un bin puro non è raggiungibile dai test di integrazione. Spostando tutto in una lib, `run.rs` diventa testabile con UI e setter finti, e `main.rs` resta di tre righe.

---

### Task 1: Scaffold, naming e CI

Il modulo `naming` non ha dipendenze, quindi è il primo pezzo che può esistere da solo. Scaffold e CI stanno qui perché sono ciò che serve per eseguirne i test.

**Files:**
- Create: `Cargo.toml`, `src/lib.rs`, `src/main.rs`, `src/naming.rs`, `.github/workflows/ci.yml`
- Test: unit test in fondo a `src/naming.rs`

**Interfaces:**
- Consumes: niente
- Produces:
  - `naming::name_key(name: &str) -> String`
  - `naming::slug(stem: &str) -> String`
  - `naming::slug_from_path(path: &Path) -> String`
  - `naming::resolve_collision(base: &str, taken: &HashSet<String>) -> Option<String>`
  - costanti `naming::MAX_SLUG_CHARS: usize = 60`, `naming::FALLBACK_NAME: &str = "Wallpaper"`, `naming::MAX_SUFFIX: u32 = 999`

- [ ] **Step 1: Creare il manifest**

`Cargo.toml`:

```toml
[package]
name = "kde-wallpaper-importer"
version = "0.1.0"
edition = "2021"
description = "Importa immagini come pacchetti wallpaper KDE dal menù contestuale di Dolphin"
license = "MIT"

[lib]
name = "kde_wallpaper_importer"
path = "src/lib.rs"

[[bin]]
name = "kde-wallpaper-import"
path = "src/main.rs"

[dependencies]
image = { version = "0.25", default-features = false, features = ["jpeg", "png", "webp", "tiff", "bmp"] }
sha2 = "0.10"
serde_json = "1"
anyhow = "1"
clap = { version = "4", features = ["derive"] }
libc = "0.2"

[dev-dependencies]
tempfile = "3"
assert_cmd = "2"
predicates = "3"
```

- [ ] **Step 2: Creare lib e bin minimi**

`src/lib.rs`:

```rust
pub mod naming;
```

`src/main.rs`:

```rust
fn main() {
    println!("segnaposto");
}
```

- [ ] **Step 3: Scrivere i test di `naming` (falliranno)**

`src/naming.rs` — solo la sezione test, il modulo è ancora vuoto:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn slug_normalizza_gli_stem() {
        let casi = [
            ("Tramonto Cala Luna", "Tramonto_Cala_Luna"),
            ("a   b", "a_b"),
            ("a_b", "a_b"),
            ("--foto--", "foto"),
            ("  foto  ", "foto"),
            ("Fjörð", "Fjörð"),
            ("archive.tar", "archive.tar"),
            ("", "Wallpaper"),
            ("___", "Wallpaper"),
            ("...", "Wallpaper"),
            ("foto\u{0}bar", "fotobar"),
            ("a/b", "ab"),
        ];
        for (input, atteso) in casi {
            assert_eq!(slug(input), atteso, "slug({input:?})");
        }
    }

    #[test]
    fn slug_tronca_a_sessanta_caratteri_su_boundary_utf8() {
        let lungo = "à".repeat(300);
        let out = slug(&lungo);
        assert_eq!(out.chars().count(), MAX_SLUG_CHARS);
        assert!(out.chars().all(|c| c == 'à'));
    }

    #[test]
    fn slug_from_path_gestisce_estensioni_e_dotfile() {
        let casi = [
            ("foto.jpg", "foto"),
            ("foto", "foto"),
            (".foto.jpg", "foto"),
            (".jpg", "Wallpaper"),
            (".", "Wallpaper"),
            ("..", "Wallpaper"),
            ("archive.tar.gz", "archive.tar"),
            ("Tramonto Cala Luna.JPEG", "Tramonto_Cala_Luna"),
        ];
        for (input, atteso) in casi {
            assert_eq!(slug_from_path(Path::new(input)), atteso, "slug_from_path({input:?})");
        }
    }

    #[test]
    fn name_key_e_case_insensitive() {
        assert_eq!(name_key("Foresta"), name_key("foresta"));
    }

    #[test]
    fn resolve_collision_restituisce_il_base_se_libero() {
        let taken = HashSet::new();
        assert_eq!(resolve_collision("foresta", &taken).as_deref(), Some("foresta"));
    }

    #[test]
    fn resolve_collision_aggiunge_suffissi_progressivi() {
        let taken: HashSet<String> =
            ["foresta", "foresta-2"].iter().map(|s| name_key(s)).collect();
        assert_eq!(resolve_collision("foresta", &taken).as_deref(), Some("foresta-3"));
    }

    #[test]
    fn resolve_collision_ignora_le_maiuscole() {
        let taken: HashSet<String> = ["Foresta"].iter().map(|s| name_key(s)).collect();
        assert_eq!(resolve_collision("foresta", &taken).as_deref(), Some("foresta-2"));
    }

    #[test]
    fn resolve_collision_si_arrende_dopo_max_suffix() {
        let mut taken: HashSet<String> = HashSet::new();
        taken.insert(name_key("x"));
        for n in 2..=MAX_SUFFIX {
            taken.insert(name_key(&format!("x-{n}")));
        }
        assert_eq!(resolve_collision("x", &taken), None);
    }
}
```

- [ ] **Step 4: Verificare che i test falliscano**

Run: `cargo test naming`
Expected: FAIL in compilazione — `cannot find function slug in this scope` e simili.

- [ ] **Step 5: Implementare `naming`**

Inserire in testa a `src/naming.rs`, prima del modulo `tests`:

```rust
use std::collections::HashSet;
use std::path::Path;

/// Lunghezza massima di un nome-pacchetto, in caratteri Unicode.
pub const MAX_SLUG_CHARS: usize = 60;
/// Nome usato quando lo stem non produce nulla di utilizzabile.
pub const FALLBACK_NAME: &str = "Wallpaper";
/// Suffisso numerico massimo tentato in caso di collisione.
pub const MAX_SUFFIX: u32 = 999;

/// Chiave di confronto per le collisioni: il match è case-insensitive perché
/// nel selettore di Plasma `foresta` e `Foresta` sarebbero indistinguibili.
pub fn name_key(name: &str) -> String {
    name.to_lowercase()
}

/// Normalizza uno stem in un nome-pacchetto. Funzione pura, nessun I/O.
///
/// `_` compare solo da questa normalizzazione, `-N` solo dalle collisioni:
/// dal nome finale si capisce sempre da dove viene il suffisso.
pub fn slug(stem: &str) -> String {
    let mut out = String::with_capacity(stem.len());
    let mut separatore_in_sospeso = false;

    for c in stem.chars() {
        if c.is_control() || c == '/' || c == '\\' {
            continue;
        }
        if c.is_whitespace() || c == '_' {
            separatore_in_sospeso = true;
            continue;
        }
        if separatore_in_sospeso && !out.is_empty() {
            out.push('_');
        }
        separatore_in_sospeso = false;
        out.push(c);
    }

    let troncato: String = out
        .trim_matches(|c: char| c == '-' || c == '_' || c == '.' || c.is_whitespace())
        .chars()
        .take(MAX_SLUG_CHARS)
        .collect();
    let troncato = troncato.trim_end_matches(['-', '_', '.']);

    if troncato.is_empty() {
        FALLBACK_NAME.to_string()
    } else {
        troncato.to_string()
    }
}

/// Estrae lo stem dal path e lo normalizza.
///
/// Un dotfile senza altri punti (`.jpg`) non ha stem: l'intero nome è
/// l'estensione, quindi si ricade su [`FALLBACK_NAME`]. Un nome non-UTF8
/// riceve lo stesso trattamento.
pub fn slug_from_path(path: &Path) -> String {
    let nome = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let stem = match nome.strip_prefix('.') {
        Some(resto) => {
            if resto.contains('.') {
                &nome[..nome.rfind('.').expect("resto contiene un punto")]
            } else {
                ""
            }
        }
        None => match nome.rfind('.') {
            Some(i) => &nome[..i],
            None => nome,
        },
    };
    slug(stem)
}

/// Primo nome libero della serie `base`, `base-2`, ... `base-MAX_SUFFIX`.
/// `taken` contiene le chiavi prodotte da [`name_key`].
pub fn resolve_collision(base: &str, taken: &HashSet<String>) -> Option<String> {
    if !taken.contains(&name_key(base)) {
        return Some(base.to_string());
    }
    for n in 2..=MAX_SUFFIX {
        let candidato = format!("{base}-{n}");
        if !taken.contains(&name_key(&candidato)) {
            return Some(candidato);
        }
    }
    None
}
```

- [ ] **Step 6: Verificare che i test passino**

Run: `cargo test naming`
Expected: PASS, 8 test.

- [ ] **Step 7: Aggiungere la CI**

`.github/workflows/ci.yml`:

```yaml
name: CI

on:
  push:
  pull_request:

jobs:
  test:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - run: cargo fmt --check
      - run: cargo clippy --all-targets -- -D warnings
      - run: cargo test
```

- [ ] **Step 8: Verificare fmt e clippy in locale**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings`
Expected: nessun output, exit 0.

- [ ] **Step 9: Commit**

```bash
git add Cargo.toml Cargo.lock src/ .github/
git commit -m "feat(naming): slug dei nomi pacchetto e risoluzione collisioni

Scaffold del crate, modulo naming con funzioni pure e workflow CI."
```

---

### Task 2: Probe delle immagini

**Files:**
- Create: `src/probe.rs`
- Modify: `src/lib.rs` (aggiungere `pub mod probe;`)
- Test: unit test in fondo a `src/probe.rs`

**Interfaces:**
- Consumes: niente
- Produces:
  - `probe::ImageInfo { width: u32, height: u32, format: image::ImageFormat }`, `Copy`, con metodo `extension(&self) -> &'static str`
  - `probe::ProbeError` con varianti `Unreadable(std::io::Error)`, `UnsupportedFormat(String)`, `Corrupt(String)`; implementa `Display` e `std::error::Error`
  - `probe::probe(path: &Path) -> Result<ImageInfo, ProbeError>`

- [ ] **Step 1: Scrivere i test (falliranno)**

`src/probe.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use tempfile::tempdir;

    fn scrivi_immagine(dir: &std::path::Path, nome: &str, w: u32, h: u32) -> std::path::PathBuf {
        let path = dir.join(nome);
        RgbImage::new(w, h).save(&path).expect("salvataggio immagine di prova");
        path
    }

    #[test]
    fn legge_dimensioni_e_formato_di_un_png() {
        let dir = tempdir().unwrap();
        let path = scrivi_immagine(dir.path(), "a.png", 640, 480);
        let info = probe(&path).expect("png valido");
        assert_eq!((info.width, info.height), (640, 480));
        assert_eq!(info.format, ImageFormat::Png);
        assert_eq!(info.extension(), "png");
    }

    #[test]
    fn usa_il_formato_rilevato_non_lestensione() {
        let dir = tempdir().unwrap();
        // Un PNG con estensione .jpg: deve vincere il contenuto.
        let path = dir.path().join("bugiardo.jpg");
        RgbImage::new(10, 20)
            .save_with_format(&path, ImageFormat::Png)
            .unwrap();
        let info = probe(&path).unwrap();
        assert_eq!(info.format, ImageFormat::Png);
        assert_eq!(info.extension(), "png");
    }

    #[test]
    fn file_mancante_e_unreadable() {
        let dir = tempdir().unwrap();
        let err = probe(&dir.path().join("assente.png")).unwrap_err();
        assert!(matches!(err, ProbeError::Unreadable(_)), "{err:?}");
    }

    #[test]
    fn contenuto_ignoto_e_unsupported_format() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("random.bin");
        std::fs::write(&path, b"non sono un'immagine, davvero").unwrap();
        let err = probe(&path).unwrap_err();
        assert!(matches!(err, ProbeError::UnsupportedFormat(_)), "{err:?}");
    }

    #[test]
    fn formato_riconosciuto_ma_non_compilato_e_unsupported_format() {
        // Il GIF è riconosciuto dai magic bytes ma la feature non è attiva.
        let dir = tempdir().unwrap();
        let path = dir.path().join("anim.gif");
        std::fs::write(&path, b"GIF89a\x01\x00\x01\x00\x00\x00\x00").unwrap();
        let err = probe(&path).unwrap_err();
        assert!(matches!(err, ProbeError::UnsupportedFormat(_)), "{err:?}");
    }

    #[test]
    fn png_troncato_e_corrupt() {
        let dir = tempdir().unwrap();
        let intero = scrivi_immagine(dir.path(), "intero.png", 64, 64);
        let bytes = std::fs::read(&intero).unwrap();
        let path = dir.path().join("troncato.png");
        std::fs::write(&path, &bytes[..12]).unwrap();
        let err = probe(&path).unwrap_err();
        assert!(matches!(err, ProbeError::Corrupt(_)), "{err:?}");
    }
}
```

- [ ] **Step 2: Verificare che i test falliscano**

Run: `cargo test probe`
Expected: FAIL in compilazione — `cannot find function probe`.

- [ ] **Step 3: Implementare `probe`**

In testa a `src/probe.rs`:

```rust
use std::fmt;
use std::path::Path;

use image::ImageFormat;

/// Dimensioni e formato rilevato di un'immagine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ImageInfo {
    pub width: u32,
    pub height: u32,
    pub format: ImageFormat,
}

impl ImageInfo {
    /// Estensione canonica del formato **rilevato**, non di quella sul disco:
    /// un JPEG rinominato `.png` finisce comunque in `<W>x<H>.jpg`.
    pub fn extension(&self) -> &'static str {
        self.format.extensions_str().first().copied().unwrap_or("img")
    }
}

#[derive(Debug)]
pub enum ProbeError {
    /// Il file non è apribile: permessi, path inesistente, I/O.
    Unreadable(std::io::Error),
    /// Formato non riconosciuto o non compilato in questa build.
    UnsupportedFormat(String),
    /// Formato riconosciuto ma contenuto non decodificabile.
    Corrupt(String),
}

impl fmt::Display for ProbeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ProbeError::Unreadable(e) => write!(f, "impossibile leggere il file ({e})"),
            ProbeError::UnsupportedFormat(d) => write!(f, "formato non supportato ({d})"),
            ProbeError::Corrupt(d) => write!(f, "immagine non valida o danneggiata ({d})"),
        }
    }
}

impl std::error::Error for ProbeError {}

/// Apre l'immagine e ne restituisce dimensioni e formato senza decodificare
/// l'intero contenuto in memoria.
pub fn probe(path: &Path) -> Result<ImageInfo, ProbeError> {
    let reader = image::ImageReader::open(path)
        .map_err(ProbeError::Unreadable)?
        .with_guessed_format()
        .map_err(ProbeError::Unreadable)?;

    let format = reader
        .format()
        .ok_or_else(|| ProbeError::UnsupportedFormat("formato non riconosciuto".to_string()))?;

    let (width, height) = reader.into_dimensions().map_err(|e| match e {
        image::ImageError::Unsupported(u) => ProbeError::UnsupportedFormat(u.to_string()),
        altro => ProbeError::Corrupt(altro.to_string()),
    })?;

    Ok(ImageInfo { width, height, format })
}
```

Aggiungere `pub mod probe;` a `src/lib.rs`.

- [ ] **Step 4: Verificare che i test passino**

Run: `cargo test probe`
Expected: PASS, 6 test.

- [ ] **Step 5: Commit**

```bash
git add src/probe.rs src/lib.rs
git commit -m "feat(probe): validazione immagini con errori tipizzati

Distingue file illeggibili, formati non supportati e contenuti corrotti;
l'estensione di destinazione deriva dal formato rilevato, non dal nome."
```

---

### Task 3: Catalogo dei wallpaper installati

**Files:**
- Create: `src/catalog.rs`
- Modify: `src/lib.rs` (aggiungere `pub mod catalog;`)
- Test: unit test in fondo a `src/catalog.rs`

**Interfaces:**
- Consumes: `naming::name_key`
- Produces:
  - `catalog::Catalog` con `scan(dest: &Path, roots: &[PathBuf]) -> Catalog`, `is_taken(&self, name: &str) -> bool`, `taken(&self) -> &HashSet<String>`, `duplicate_of(&self, sha256: &str) -> Option<&str>`, `mark_taken(&mut self, name: &str)`, `record_import(&mut self, name: &str, sha256: &str)`
  - `catalog::default_dest() -> PathBuf`
  - `catalog::wallpaper_roots(dest: &Path) -> Vec<PathBuf>`
  - `catalog::wallpaper_roots_from(dest: &Path, data_home: &Path, data_dirs: &str) -> Vec<PathBuf>`

- [ ] **Step 1: Scrivere i test (falliranno)**

`src/catalog.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn pacchetto(root: &Path, nome: &str, sha: Option<&str>) {
        let dir = root.join(nome);
        std::fs::create_dir_all(dir.join("contents/images")).unwrap();
        let json = match sha {
            Some(s) => format!(
                r#"{{"KPlugin":{{"Id":"{nome}"}},"X-KWI":{{"SourceSha256":"{s}"}}}}"#
            ),
            None => format!(r#"{{"KPlugin":{{"Id":"{nome}"}}}}"#),
        };
        std::fs::write(dir.join("metadata.json"), json).unwrap();
    }

    #[test]
    fn raccoglie_i_nomi_da_tutte_le_root() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("utente");
        let sistema = tmp.path().join("sistema");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::create_dir_all(&sistema).unwrap();
        pacchetto(&dest, "Foresta", None);
        pacchetto(&sistema, "Altai", None);

        let cat = Catalog::scan(&dest, &[dest.clone(), sistema.clone()]);
        assert!(cat.is_taken("Foresta"));
        assert!(cat.is_taken("altai"), "il confronto è case-insensitive");
        assert!(!cat.is_taken("Mare"));
    }

    #[test]
    fn il_dedup_guarda_solo_la_destinazione() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("utente");
        let sistema = tmp.path().join("sistema");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::create_dir_all(&sistema).unwrap();
        pacchetto(&dest, "Foresta", Some("aaa"));
        pacchetto(&sistema, "Altrove", Some("bbb"));

        let cat = Catalog::scan(&dest, &[dest.clone(), sistema.clone()]);
        assert_eq!(cat.duplicate_of("aaa"), Some("Foresta"));
        assert_eq!(cat.duplicate_of("bbb"), None);
    }

    #[test]
    fn ignora_directory_nascoste_e_root_inesistenti() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("utente");
        std::fs::create_dir_all(dest.join(".kwi-tmp-1-0")).unwrap();
        let cat = Catalog::scan(&dest, &[dest.clone(), tmp.path().join("non-esiste")]);
        assert!(!cat.is_taken(".kwi-tmp-1-0"));
        assert!(cat.taken().is_empty());
    }

    #[test]
    fn pacchetto_senza_hash_occupa_il_nome_ma_non_deduplica() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("utente");
        std::fs::create_dir_all(&dest).unwrap();
        pacchetto(&dest, "Altai", None);
        let cat = Catalog::scan(&dest, &[dest.clone()]);
        assert!(cat.is_taken("Altai"));
        assert_eq!(cat.duplicate_of("aaa"), None);
    }

    #[test]
    fn record_import_e_mark_taken_aggiornano_lo_stato() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("utente");
        std::fs::create_dir_all(&dest).unwrap();
        let mut cat = Catalog::scan(&dest, &[dest.clone()]);

        cat.record_import("Mare", "ccc");
        assert!(cat.is_taken("mare"));
        assert_eq!(cat.duplicate_of("ccc"), Some("Mare"));

        cat.mark_taken("Cielo");
        assert!(cat.is_taken("cielo"));
        assert_eq!(cat.duplicate_of(""), None, "mark_taken non tocca la mappa hash");
    }

    #[test]
    fn wallpaper_roots_from_espande_e_deduplica() {
        let dest = Path::new("/home/u/.local/share/wallpapers");
        let roots = wallpaper_roots_from(
            dest,
            Path::new("/home/u/.local/share"),
            "/home/u/.local/share:/usr/share:/usr/share",
        );
        assert!(roots.contains(&PathBuf::from("/home/u/.local/share/wallpapers")));
        assert!(roots.contains(&PathBuf::from("/usr/share/wallpapers")));
        assert_eq!(
            roots.iter().filter(|r| *r == dest).count(),
            1,
            "nessun duplicato"
        );
    }

    #[test]
    fn wallpaper_roots_from_usa_il_default_xdg_se_data_dirs_e_vuota() {
        let dest = Path::new("/tmp/dest");
        let roots = wallpaper_roots_from(dest, Path::new("/home/u/.local/share"), "");
        assert!(roots.contains(&PathBuf::from("/usr/local/share/wallpapers")));
        assert!(roots.contains(&PathBuf::from("/usr/share/wallpapers")));
    }
}
```

- [ ] **Step 2: Verificare che i test falliscano**

Run: `cargo test catalog`
Expected: FAIL in compilazione — `cannot find type Catalog`.

- [ ] **Step 3: Implementare `catalog`**

In testa a `src/catalog.rs`:

```rust
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::naming::name_key;

/// Nomi già occupati e mappa hash → pacchetto della root di destinazione.
pub struct Catalog {
    taken: HashSet<String>,
    by_hash: HashMap<String, String>,
}

impl Catalog {
    /// Scansiona `roots` per i nomi occupati e la sola `dest` per gli hash.
    ///
    /// Le root inesistenti vengono ignorate: `$XDG_DATA_DIRS` contiene spesso
    /// path che non esistono su questa macchina.
    pub fn scan(dest: &Path, roots: &[PathBuf]) -> Self {
        let mut taken = HashSet::new();
        let mut by_hash = HashMap::new();

        for root in roots {
            let Ok(entries) = std::fs::read_dir(root) else {
                continue;
            };
            for entry in entries.flatten() {
                if !entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    continue;
                }
                let Ok(nome) = entry.file_name().into_string() else {
                    continue;
                };
                // Salta le tmp dir e ogni altra directory nascosta.
                if nome.starts_with('.') {
                    continue;
                }
                taken.insert(name_key(&nome));
                if root == dest {
                    if let Some(hash) = leggi_source_hash(&entry.path()) {
                        by_hash.insert(hash, nome);
                    }
                }
            }
        }

        Self { taken, by_hash }
    }

    pub fn is_taken(&self, name: &str) -> bool {
        self.taken.contains(&name_key(name))
    }

    pub fn taken(&self) -> &HashSet<String> {
        &self.taken
    }

    pub fn duplicate_of(&self, sha256: &str) -> Option<&str> {
        self.by_hash.get(sha256).map(String::as_str)
    }

    /// Occupa un nome senza registrare un hash: serve quando il rename
    /// atomico scopre che il nome è stato preso da un altro processo.
    pub fn mark_taken(&mut self, name: &str) {
        self.taken.insert(name_key(name));
    }

    /// Registra un import appena riuscito, così i file successivi dello
    /// stesso batch vedono sia il nome occupato sia l'hash.
    pub fn record_import(&mut self, name: &str, sha256: &str) {
        self.taken.insert(name_key(name));
        self.by_hash.insert(sha256.to_string(), name.to_string());
    }
}

fn leggi_source_hash(pacchetto: &Path) -> Option<String> {
    let testo = std::fs::read_to_string(pacchetto.join("metadata.json")).ok()?;
    let valore: serde_json::Value = serde_json::from_str(&testo).ok()?;
    valore
        .get("X-KWI")?
        .get("SourceSha256")?
        .as_str()
        .map(str::to_string)
}

fn data_home() -> PathBuf {
    match std::env::var_os("XDG_DATA_HOME") {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => PathBuf::from(std::env::var_os("HOME").unwrap_or_default()).join(".local/share"),
    }
}

/// `${XDG_DATA_HOME:-$HOME/.local/share}/wallpapers`
pub fn default_dest() -> PathBuf {
    data_home().join("wallpapers")
}

/// Legge l'ambiente e delega a [`wallpaper_roots_from`].
pub fn wallpaper_roots(dest: &Path) -> Vec<PathBuf> {
    let data_dirs = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    wallpaper_roots_from(dest, &data_home(), &data_dirs)
}

/// Tutte le directory `wallpapers` da controllare per le collisioni di nome.
///
/// Include le root di sistema perché un pacchetto utente con lo stesso Id di
/// uno di sistema lo maschera nella risoluzione KPackage, facendo sparire il
/// wallpaper originale dal selettore di Plasma.
pub fn wallpaper_roots_from(dest: &Path, data_home: &Path, data_dirs: &str) -> Vec<PathBuf> {
    let mut roots = vec![dest.to_path_buf(), data_home.join("wallpapers")];

    let dirs = if data_dirs.is_empty() {
        "/usr/local/share:/usr/share"
    } else {
        data_dirs
    };
    for d in dirs.split(':').filter(|s| !s.is_empty()) {
        roots.push(Path::new(d).join("wallpapers"));
    }

    roots.sort();
    roots.dedup();
    roots
}
```

Aggiungere `pub mod catalog;` a `src/lib.rs`.

- [ ] **Step 4: Verificare che i test passino**

Run: `cargo test catalog`
Expected: PASS, 7 test.

- [ ] **Step 5: Commit**

```bash
git add src/catalog.rs src/lib.rs
git commit -m "feat(catalog): nomi occupati su tutte le root XDG e dedup per hash

Il controllo nomi copre anche /usr/share/wallpapers: un pacchetto utente
omonimo maschererebbe quello di sistema nel selettore di Plasma."
```

---

### Task 4: Scrittura atomica del pacchetto

**Files:**
- Create: `src/package.rs`
- Modify: `src/lib.rs` (aggiungere `pub mod package;`)
- Test: unit test in fondo a `src/package.rs`

**Interfaces:**
- Consumes: `probe::ImageInfo`
- Produces:
  - `package::PackageSpec<'a> { source: &'a Path, name: &'a str, info: &'a ImageInfo, sha256: &'a str }`
  - `package::WriteError` con varianti `NameTaken` e `Other(anyhow::Error)`; implementa `Display`
  - `package::write(spec: &PackageSpec, dest_dir: &Path) -> Result<(), WriteError>`
  - `package::sha256_file(path: &Path) -> std::io::Result<String>`
  - `package::metadata_json(name: &str, sha256: &str, source: &Path, when: SystemTime) -> String`
  - `package::iso8601_utc(t: SystemTime) -> String`
  - `package::sweep_stale_tmp(root: &Path, max_age: Duration)`
  - costanti `package::SCREENSHOT_MAX: u32 = 1280`, `package::TMP_PREFIX: &str = ".kwi-tmp-"`

- [ ] **Step 1: Scrivere i test (falliranno)**

`src/package.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use std::time::Duration;
    use tempfile::tempdir;

    fn immagine(dir: &Path, nome: &str, w: u32, h: u32) -> PathBuf {
        let path = dir.join(nome);
        RgbImage::new(w, h).save(&path).unwrap();
        path
    }

    fn info(w: u32, h: u32) -> ImageInfo {
        ImageInfo { width: w, height: h, format: ImageFormat::Png }
    }

    #[test]
    fn iso8601_formatta_epoch_noti() {
        assert_eq!(iso8601_utc(UNIX_EPOCH), "1970-01-01T00:00:00Z");
        assert_eq!(
            iso8601_utc(UNIX_EPOCH + Duration::from_secs(1_700_000_000)),
            "2023-11-14T22:13:20Z"
        );
    }

    #[test]
    fn sha256_file_calcola_hash_noti() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("vuoto.bin");
        std::fs::write(&path, b"").unwrap();
        assert_eq!(
            sha256_file(&path).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn metadata_json_contiene_i_campi_attesi() {
        let json = metadata_json(
            "Tramonto_Cala_Luna",
            "abc123",
            Path::new("/home/u/foto.jpg"),
            UNIX_EPOCH,
        );
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["KPlugin"]["Id"], "Tramonto_Cala_Luna");
        assert_eq!(v["KPlugin"]["Name"], "Tramonto Cala Luna");
        assert_eq!(v["KPlugin"]["License"], "Unknown");
        assert_eq!(v["X-KWI"]["SourceSha256"], "abc123");
        assert_eq!(v["X-KWI"]["SourcePath"], "/home/u/foto.jpg");
        assert_eq!(v["X-KWI"]["ImportedAt"], "1970-01-01T00:00:00Z");
        assert!(v["KPlugin"]["Authors"].is_null(), "non attribuiamo l'immagine all'utente");
    }

    #[test]
    fn write_crea_la_struttura_kpackage() {
        let dir = tempdir().unwrap();
        let src = immagine(dir.path(), "foto.png", 1600, 900);
        let dest_root = dir.path().join("wallpapers");
        let target = dest_root.join("Foto");

        let i = info(1600, 900);
        let spec = PackageSpec { source: &src, name: "Foto", info: &i, sha256: "abc" };
        write(&spec, &target).expect("scrittura pacchetto");

        assert!(target.join("metadata.json").is_file());
        assert!(target.join("contents/screenshot.png").is_file());
        let originale = target.join("contents/images/1600x900.png");
        assert!(originale.is_file());
        assert_eq!(
            std::fs::read(&originale).unwrap(),
            std::fs::read(&src).unwrap(),
            "l'originale è copiato byte per byte"
        );
    }

    #[test]
    fn lo_screenshot_e_ridimensionato_solo_se_serve() {
        let dir = tempdir().unwrap();
        let dest_root = dir.path().join("wallpapers");

        let grande = immagine(dir.path(), "grande.png", 2560, 1440);
        let i = info(2560, 1440);
        let spec = PackageSpec { source: &grande, name: "Grande", info: &i, sha256: "a" };
        write(&spec, &dest_root.join("Grande")).unwrap();
        let s = image::open(dest_root.join("Grande/contents/screenshot.png")).unwrap();
        assert_eq!(s.width(), SCREENSHOT_MAX);
        assert_eq!(s.height(), 720);

        let piccola = immagine(dir.path(), "piccola.png", 300, 200);
        let i = info(300, 200);
        let spec = PackageSpec { source: &piccola, name: "Piccola", info: &i, sha256: "b" };
        write(&spec, &dest_root.join("Piccola")).unwrap();
        let s = image::open(dest_root.join("Piccola/contents/screenshot.png")).unwrap();
        assert_eq!((s.width(), s.height()), (300, 200), "niente upscaling");
    }

    #[test]
    fn write_accetta_unimmagine_con_estensione_sbagliata() {
        // `probe` rileva il formato dal contenuto e `write` deve usare lo stesso
        // criterio: un JPEG chiamato .png, o un file senza estensione, passa
        // `probe` e non deve far abortire l'import in fase di screenshot.
        let dir = tempdir().unwrap();
        let src = dir.path().join("bugiardo.png");
        RgbImage::new(1600, 900)
            .save_with_format(&src, ImageFormat::Jpeg)
            .unwrap();

        let i = ImageInfo {
            width: 1600,
            height: 900,
            format: ImageFormat::Jpeg,
        };
        let dest_root = dir.path().join("wallpapers");
        let spec = PackageSpec {
            source: &src,
            name: "Bugiardo",
            info: &i,
            sha256: "abc",
        };
        write(&spec, &dest_root.join("Bugiardo")).expect("formato dedotto dal contenuto");

        assert!(dest_root
            .join("Bugiardo/contents/images/1600x900.jpg")
            .is_file());
        assert!(dest_root.join("Bugiardo/contents/screenshot.png").is_file());
    }

    #[test]
    fn write_su_nome_esistente_restituisce_name_taken() {
        let dir = tempdir().unwrap();
        let src = immagine(dir.path(), "foto.png", 100, 100);
        let dest_root = dir.path().join("wallpapers");
        let target = dest_root.join("Foto");
        std::fs::create_dir_all(&target).unwrap();

        let i = info(100, 100);
        let spec = PackageSpec { source: &src, name: "Foto", info: &i, sha256: "abc" };
        assert!(matches!(write(&spec, &target), Err(WriteError::NameTaken)));
    }

    #[test]
    fn un_fallimento_non_lascia_tmp_dir() {
        let dir = tempdir().unwrap();
        let dest_root = dir.path().join("wallpapers");
        std::fs::create_dir_all(&dest_root).unwrap();
        let assente = dir.path().join("assente.png");

        let i = info(100, 100);
        let spec = PackageSpec { source: &assente, name: "Foto", info: &i, sha256: "abc" };
        assert!(write(&spec, &dest_root.join("Foto")).is_err());

        let residui: Vec<_> = std::fs::read_dir(&dest_root)
            .unwrap()
            .flatten()
            .filter(|e| e.file_name().to_string_lossy().starts_with(TMP_PREFIX))
            .collect();
        assert!(residui.is_empty(), "tmp dir non ripulita: {residui:?}");
    }

    #[test]
    fn sweep_rimuove_solo_le_tmp_dir_vecchie() {
        let dir = tempdir().unwrap();
        let root = dir.path();
        let vecchia = root.join(format!("{TMP_PREFIX}1-0"));
        let recente = root.join(format!("{TMP_PREFIX}2-0"));
        let estranea_recente = root.join("Foresta");
        // Un wallpaper vero dell'utente e' quasi sempre piu' vecchio di 24h:
        // senza questa dir il filtro sul prefisso non sarebbe mai determinante
        // e il test resterebbe verde anche rimuovendolo.
        let estranea_vecchia = root.join("Foresta_Antica");
        for d in [&vecchia, &recente, &estranea_recente, &estranea_vecchia] {
            std::fs::create_dir_all(d).unwrap();
        }
        let due_giorni_fa = std::time::SystemTime::now() - Duration::from_secs(48 * 3600);
        filetime_set(&vecchia, due_giorni_fa);
        filetime_set(&estranea_vecchia, due_giorni_fa);

        sweep_stale_tmp(root, Duration::from_secs(24 * 3600));

        assert!(!vecchia.exists(), "tmp dir vecchia: va rimossa");
        assert!(recente.exists(), "tmp dir recente: va risparmiata");
        assert!(estranea_recente.exists(), "dir senza prefisso: va risparmiata");
        assert!(
            estranea_vecchia.exists(),
            "dir senza prefisso, anche vecchia: e' un wallpaper dell'utente, non va toccata"
        );
    }

    #[test]
    fn la_tmp_dir_nasce_nella_root_di_destinazione() {
        // Se la tmp dir finisse altrove (es. /tmp) il rename attraverserebbe
        // filesystem e non sarebbe piu' atomico. Su una macchina con un solo
        // filesystem nessun altro test se ne accorgerebbe.
        let dir = tempdir().unwrap();
        let root = dir.path();
        let tmp = TmpDir::new(root).unwrap();
        assert_eq!(tmp.path().parent(), Some(root));
        assert!(tmp
            .path()
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .starts_with(TMP_PREFIX));
    }

    /// Imposta la mtime di una directory senza dipendenze aggiuntive.
    fn filetime_set(path: &Path, when: std::time::SystemTime) {
        use std::os::unix::ffi::OsStrExt;
        let secs = when.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let times = [
            libc::timespec { tv_sec: secs, tv_nsec: 0 },
            libc::timespec { tv_sec: secs, tv_nsec: 0 },
        ];
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0) };
        assert_eq!(rc, 0, "utimensat fallita");
    }
}
```

- [ ] **Step 2: Verificare che i test falliscano**

Run: `cargo test package`
Expected: FAIL in compilazione — `cannot find function write`.

- [ ] **Step 3: Implementare hash, timestamp e metadata**

In testa a `src/package.rs`:

```rust
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::anyhow;
use sha2::{Digest, Sha256};

use crate::probe::ImageInfo;

/// Lato lungo massimo dello screenshot di anteprima.
pub const SCREENSHOT_MAX: u32 = 1280;
/// Prefisso delle directory temporanee, nascoste per non finire nel catalogo.
pub const TMP_PREFIX: &str = ".kwi-tmp-";

/// Digest esadecimale SHA-256 del contenuto del file.
pub fn sha256_file(path: &Path) -> io::Result<String> {
    let mut file = fs::File::open(path)?;
    let mut hasher = Sha256::new();
    io::copy(&mut file, &mut hasher)?;
    Ok(format!("{:x}", hasher.finalize()))
}

/// Timestamp RFC 3339 in UTC, senza dipendere da un crate di date.
pub fn iso8601_utc(t: SystemTime) -> String {
    let secs = t
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let giorni = secs.div_euclid(86_400);
    let sec_del_giorno = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(giorni);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        sec_del_giorno / 3600,
        (sec_del_giorno % 3600) / 60,
        sec_del_giorno % 60
    )
}

/// Conversione giorni-dall'epoch → data civile (algoritmo di Howard Hinnant).
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719_468;
    // div_euclid arrotonda verso il basso anche per z negativi, quindi
    // l'aggiustamento manuale dell'algoritmo originale non serve.
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u32;
    (if m <= 2 { y + 1 } else { y }, m, d)
}

/// Contenuto di `metadata.json`. `Authors` è volutamente assente: non ha senso
/// attribuire l'immagine all'utente che la importa.
pub fn metadata_json(name: &str, sha256: &str, source: &Path, when: SystemTime) -> String {
    let valore = serde_json::json!({
        "KPlugin": {
            "Id": name,
            "Name": name.replace('_', " "),
            "License": "Unknown",
        },
        "X-KWI": {
            "SourceSha256": sha256,
            "SourcePath": source.display().to_string(),
            "ImportedAt": iso8601_utc(when),
        }
    });
    let mut s = serde_json::to_string_pretty(&valore).expect("json serializzabile");
    s.push('\n');
    s
}
```

- [ ] **Step 4: Verificare i test puri**

Run: `cargo test package::tests::iso8601 package::tests::sha256_file package::tests::metadata_json`
Expected: PASS per i tre test; gli altri ancora falliscono in compilazione.

Se la compilazione dei test blocca l'esecuzione, procedere allo Step 5 e verificare tutto insieme allo Step 7.

- [ ] **Step 5: Implementare la tmp dir e il rename atomico**

Aggiungere a `src/package.rs`:

```rust
/// Directory temporanea auto-cancellante. `disarm()` la disinnesca dopo che il
/// rename l'ha già spostata a destinazione.
struct TmpDir {
    path: PathBuf,
    armata: bool,
}

impl TmpDir {
    fn new(root: &Path) -> io::Result<Self> {
        for n in 0..1000u32 {
            let path = root.join(format!("{TMP_PREFIX}{}-{n}", std::process::id()));
            match fs::create_dir(&path) {
                Ok(()) => return Ok(Self { path, armata: true }),
                Err(e) if e.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(e) => return Err(e),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            "nessuna directory temporanea libera",
        ))
    }

    fn path(&self) -> &Path {
        &self.path
    }

    fn disarm(mut self) {
        self.armata = false;
    }
}

impl Drop for TmpDir {
    fn drop(&mut self) {
        if self.armata {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}

/// `rename(2)` che fallisce con `EEXIST` invece di sovrascrivere.
///
/// Su kernel o filesystem senza `renameat2` si degrada a check-then-rename,
/// che lascia una finestra di race trascurabile per questo caso d'uso.
fn rename_noreplace(from: &Path, to: &Path) -> io::Result<()> {
    use std::os::unix::ffi::OsStrExt;

    let from_c = std::ffi::CString::new(from.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let to_c = std::ffi::CString::new(to.as_os_str().as_bytes())
        .map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;

    let rc = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from_c.as_ptr(),
            libc::AT_FDCWD,
            to_c.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    };
    if rc == 0 {
        return Ok(());
    }

    let err = io::Error::last_os_error();
    match err.raw_os_error() {
        Some(libc::EINVAL) | Some(libc::ENOSYS) | Some(libc::EOPNOTSUPP) => {
            if to.symlink_metadata().is_ok() {
                return Err(io::Error::from_raw_os_error(libc::EEXIST));
            }
            fs::rename(from, to)
        }
        _ => Err(err),
    }
}

/// Rimuove le tmp dir più vecchie di `max_age`, lasciate da crash precedenti.
pub fn sweep_stale_tmp(root: &Path, max_age: Duration) {
    let Ok(entries) = fs::read_dir(root) else {
        return;
    };
    let ora = SystemTime::now();
    for entry in entries.flatten() {
        let nome = entry.file_name();
        let Some(nome) = nome.to_str() else { continue };
        if !nome.starts_with(TMP_PREFIX) {
            continue;
        }
        let vecchia = entry
            .metadata()
            .ok()
            .and_then(|m| m.modified().ok())
            .and_then(|t| ora.duration_since(t).ok())
            .map(|eta| eta > max_age)
            .unwrap_or(false);
        if vecchia {
            let _ = fs::remove_dir_all(entry.path());
        }
    }
}
```

- [ ] **Step 6: Implementare `write`**

Aggiungere a `src/package.rs`:

```rust
/// Cosa scrivere e con quale identità.
pub struct PackageSpec<'a> {
    pub source: &'a Path,
    pub name: &'a str,
    pub info: &'a ImageInfo,
    pub sha256: &'a str,
}

#[derive(Debug)]
pub enum WriteError {
    /// Il nome è stato occupato tra la risoluzione e il rename.
    NameTaken,
    Other(anyhow::Error),
}

impl std::fmt::Display for WriteError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            WriteError::NameTaken => write!(f, "nome già occupato"),
            WriteError::Other(e) => write!(f, "{e}"),
        }
    }
}

impl std::error::Error for WriteError {}

fn other<E: Into<anyhow::Error>>(e: E) -> WriteError {
    WriteError::Other(e.into())
}

/// Costruisce il pacchetto in una tmp dir e lo sposta atomicamente in
/// `dest_dir`: o la directory finale appare completa, o non appare affatto.
pub fn write(spec: &PackageSpec, dest_dir: &Path) -> Result<(), WriteError> {
    let root = dest_dir
        .parent()
        .ok_or_else(|| other(anyhow!("destinazione senza directory padre")))?;
    fs::create_dir_all(root).map_err(other)?;

    // La tmp dir sta nella stessa root del pacchetto: il rename resta atomico
    // perché non attraversa filesystem.
    let tmp = TmpDir::new(root).map_err(other)?;

    let images = tmp.path().join("contents/images");
    fs::create_dir_all(&images).map_err(other)?;

    let nome_immagine = format!(
        "{}x{}.{}",
        spec.info.width,
        spec.info.height,
        spec.info.extension()
    );
    fs::copy(spec.source, images.join(&nome_immagine)).map_err(other)?;

    scrivi_screenshot(spec.source, &tmp.path().join("contents/screenshot.png"))
        .map_err(WriteError::Other)?;

    fs::write(
        tmp.path().join("metadata.json"),
        metadata_json(spec.name, spec.sha256, spec.source, SystemTime::now()),
    )
    .map_err(other)?;

    match rename_noreplace(tmp.path(), dest_dir) {
        Ok(()) => {
            tmp.disarm();
            Ok(())
        }
        Err(e) if e.raw_os_error() == Some(libc::EEXIST) => Err(WriteError::NameTaken),
        Err(e) => Err(other(e)),
    }
}

fn scrivi_screenshot(source: &Path, dest: &Path) -> anyhow::Result<()> {
    // `image::open` deduce il formato dalla sola estensione del path: userebbe
    // un criterio opposto a quello di `probe`, e farebbe abortire l'import di
    // un JPEG chiamato .png o di un file senza estensione.
    let img = image::ImageReader::open(source)?
        .with_guessed_format()?
        .decode()?;
    let out = if img.width() > SCREENSHOT_MAX || img.height() > SCREENSHOT_MAX {
        img.resize(
            SCREENSHOT_MAX,
            SCREENSHOT_MAX,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        img
    };
    out.save_with_format(dest, image::ImageFormat::Png)?;
    Ok(())
}
```

Aggiungere `pub mod package;` a `src/lib.rs`.

- [ ] **Step 7: Verificare che i test passino**

Run: `cargo test package`
Expected: PASS, 10 test.

- [ ] **Step 8: Commit**

```bash
git add src/package.rs src/lib.rs
git commit -m "feat(package): scrittura atomica del KPackage

Costruzione in tmp dir più renameat2(RENAME_NOREPLACE): nessun pacchetto
parziale e nessuna sovrascrittura silenziosa. Timestamp RFC 3339 calcolato
senza crate di date."
```

---

### Task 5: UI e applicazione dello sfondo

Entrambi i moduli sono sottili wrapper su processi esterni, e nessuno dei due deve poter essere eseguito davvero dai test: stanno insieme perché condividono lo stesso motivo di esistere, cioè essere sostituibili con dei finti.

**Files:**
- Create: `src/ui.rs`, `src/apply.rs`
- Modify: `src/lib.rs` (aggiungere `pub mod ui;` e `pub mod apply;`)
- Test: unit test in fondo a `src/ui.rs` e a `src/apply.rs`

**Interfaces:**
- Consumes: niente
- Produces:
  - `ui::Ui` trait: `fn confirm(&self, title: &str, message: &str) -> bool`, `fn notify(&self, summary: &str, body: &str, critical: bool)`
  - `ui::KdeUi`, `ui::SilentUi` (unit struct)
  - `ui::testing::FakeUi` con `FakeUi::new(risposta_confirm: bool)`, `fn confirms(&self) -> Vec<String>`, `fn notifications(&self) -> Vec<(String, String, bool)>`
  - `apply::WallpaperSetter` trait: `fn apply(&self, package: &Path, fill_mode: Option<&str>) -> anyhow::Result<()>`
  - `apply::plasma_args(package: &Path, fill_mode: Option<&str>) -> Vec<OsString>`
  - `apply::PlasmaSetter` (unit struct)
  - `apply::testing::FakeSetter` con `FakeSetter::new()`, `fn calls(&self) -> Vec<(PathBuf, Option<String>)>`, `FakeSetter::failing()`

- [ ] **Step 1: Scrivere i test (falliranno)**

`src/ui.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::testing::FakeUi;
    use super::*;

    #[test]
    fn silent_ui_conferma_sempre_e_non_notifica() {
        let ui = SilentUi;
        assert!(ui.confirm("titolo", "messaggio"));
        ui.notify("s", "b", true); // non deve fare nulla né andare in panico
    }

    #[test]
    fn fake_ui_registra_conferme_e_notifiche() {
        let ui = FakeUi::new(false);
        assert!(!ui.confirm("t", "davvero?"));
        ui.notify("Esito", "1 importati", false);

        assert_eq!(ui.confirms(), vec!["davvero?".to_string()]);
        assert_eq!(
            ui.notifications(),
            vec![("Esito".to_string(), "1 importati".to_string(), false)]
        );
    }
}
```

- [ ] **Step 2: Verificare che i test falliscano**

Run: `cargo test ui`
Expected: FAIL in compilazione — `cannot find type SilentUi`.

- [ ] **Step 3: Implementare `ui`**

In testa a `src/ui.rs`:

```rust
use std::process::Command;

/// Interazione con l'utente. Dietro trait perché i test non devono aprire
/// finestre: `SilentUi` e `testing::FakeUi` la sostituiscono.
pub trait Ui {
    /// Domanda bloccante sì/no. `true` significa «procedi».
    fn confirm(&self, title: &str, message: &str) -> bool;
    /// Notifica non bloccante di esito.
    fn notify(&self, summary: &str, body: &str, critical: bool);
}

/// Implementazione reale su `kdialog` e `notify-send`.
pub struct KdeUi;

impl Ui for KdeUi {
    fn confirm(&self, title: &str, message: &str) -> bool {
        match Command::new("kdialog")
            .arg("--title")
            .arg(title)
            .arg("--yesno")
            .arg(message)
            .status()
        {
            Ok(s) => s.success(),
            Err(e) => {
                eprintln!("kdialog non disponibile ({e}): procedo senza chiedere");
                true
            }
        }
    }

    fn notify(&self, summary: &str, body: &str, critical: bool) {
        let urgenza = if critical { "critical" } else { "normal" };
        let _ = Command::new("notify-send")
            .args([
                "-a",
                "Wallpaper Importer",
                "-i",
                "preferences-desktop-wallpaper",
                "-u",
                urgenza,
            ])
            .arg(summary)
            .arg(body)
            .status();
    }
}

/// Usata con `--no-ui`: nessun dialogo, nessuna notifica, conferma implicita.
pub struct SilentUi;

impl Ui for SilentUi {
    fn confirm(&self, _title: &str, _message: &str) -> bool {
        true
    }
    fn notify(&self, _summary: &str, _body: &str, _critical: bool) {}
}

#[cfg(test)]
pub mod testing {
    use super::Ui;
    use std::cell::RefCell;

    /// UI finta che registra ogni interazione.
    pub struct FakeUi {
        risposta: bool,
        confirms: RefCell<Vec<String>>,
        notifications: RefCell<Vec<(String, String, bool)>>,
    }

    impl FakeUi {
        pub fn new(risposta_confirm: bool) -> Self {
            Self {
                risposta: risposta_confirm,
                confirms: RefCell::new(Vec::new()),
                notifications: RefCell::new(Vec::new()),
            }
        }

        pub fn confirms(&self) -> Vec<String> {
            self.confirms.borrow().clone()
        }

        pub fn notifications(&self) -> Vec<(String, String, bool)> {
            self.notifications.borrow().clone()
        }
    }

    impl Ui for FakeUi {
        fn confirm(&self, _title: &str, message: &str) -> bool {
            self.confirms.borrow_mut().push(message.to_string());
            self.risposta
        }

        fn notify(&self, summary: &str, body: &str, critical: bool) {
            self.notifications.borrow_mut().push((
                summary.to_string(),
                body.to_string(),
                critical,
            ));
        }
    }
}
```

- [ ] **Step 4: Scrivere i test di `apply` (falliranno)**

`src/apply.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plasma_args_senza_fill_mode_passa_solo_il_pacchetto() {
        assert_eq!(
            plasma_args(Path::new("/w/Foresta"), None),
            vec![OsString::from("/w/Foresta")]
        );
    }

    #[test]
    fn plasma_args_antepone_il_fill_mode() {
        assert_eq!(
            plasma_args(Path::new("/w/Foresta"), Some("stretch")),
            vec![
                OsString::from("--fill-mode"),
                OsString::from("stretch"),
                OsString::from("/w/Foresta"),
            ]
        );
    }

    #[test]
    fn plasma_args_tiene_insieme_i_path_con_spazi() {
        // Un solo argomento, non due: nessuno shell quoting di mezzo.
        assert_eq!(
            plasma_args(Path::new("/w/Cala Luna"), None),
            vec![OsString::from("/w/Cala Luna")]
        );
    }
}
```

- [ ] **Step 5: Implementare `apply`**

In testa a `src/apply.rs`:

```rust
use std::ffi::OsString;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context};

/// Imposta lo sfondo della sessione. Dietro trait perché i test non devono
/// cambiare davvero il wallpaper della macchina su cui girano.
pub trait WallpaperSetter {
    fn apply(&self, package: &Path, fill_mode: Option<&str>) -> anyhow::Result<()>;
}

/// Argomenti per `plasma-apply-wallpaperimage`.
///
/// Estratta come funzione pura perché è l'unica parte di [`PlasmaSetter`]
/// verificabile senza cambiare davvero lo sfondo della macchina di test.
pub fn plasma_args(package: &Path, fill_mode: Option<&str>) -> Vec<OsString> {
    let mut args = Vec::with_capacity(3);
    if let Some(m) = fill_mode {
        args.push(OsString::from("--fill-mode"));
        args.push(OsString::from(m));
    }
    args.push(package.as_os_str().to_os_string());
    args
}

/// Implementazione reale su `plasma-apply-wallpaperimage`, che accetta sia un
/// file immagine sia una directory di pacchetto.
pub struct PlasmaSetter;

impl WallpaperSetter for PlasmaSetter {
    fn apply(&self, package: &Path, fill_mode: Option<&str>) -> anyhow::Result<()> {
        let status = Command::new("plasma-apply-wallpaperimage")
            .args(plasma_args(package, fill_mode))
            .status()
            .context("plasma-apply-wallpaperimage non disponibile")?;
        if !status.success() {
            bail!("plasma-apply-wallpaperimage è uscito con {status}");
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod testing {
    use super::WallpaperSetter;
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    pub struct FakeSetter {
        fallisce: bool,
        calls: RefCell<Vec<(PathBuf, Option<String>)>>,
    }

    impl FakeSetter {
        pub fn new() -> Self {
            Self { fallisce: false, calls: RefCell::new(Vec::new()) }
        }

        pub fn failing() -> Self {
            Self { fallisce: true, calls: RefCell::new(Vec::new()) }
        }

        pub fn calls(&self) -> Vec<(PathBuf, Option<String>)> {
            self.calls.borrow().clone()
        }
    }

    impl Default for FakeSetter {
        fn default() -> Self {
            Self::new()
        }
    }

    impl WallpaperSetter for FakeSetter {
        fn apply(&self, package: &Path, fill_mode: Option<&str>) -> anyhow::Result<()> {
            self.calls
                .borrow_mut()
                .push((package.to_path_buf(), fill_mode.map(str::to_string)));
            if self.fallisce {
                anyhow::bail!("setter finto in errore");
            }
            Ok(())
        }
    }
}
```

Aggiungere `pub mod apply;` e `pub mod ui;` a `src/lib.rs`.

- [ ] **Step 6: Verificare che i test passino**

Run: `cargo test ui:: && cargo test apply::`
Expected: PASS, 2 test per `ui` e 3 per `apply`.

- [ ] **Step 7: Commit**

```bash
git add src/ui.rs src/apply.rs src/lib.rs
git commit -m "feat(ui,apply): kdialog, notify-send e plasma dietro trait

Le implementazioni finte permettono di testare l'orchestrazione senza
aprire finestre né toccare lo sfondo della macchina di test. La
costruzione degli argomenti di plasma è una funzione pura testabile."
```

---

### Task 6: CLI, orchestrazione e test end-to-end

**Files:**
- Create: `src/cli.rs`, `src/run.rs`, `tests/common/mod.rs`, `tests/import.rs`
- Modify: `src/lib.rs` (aggiungere `pub mod cli;` e `pub mod run;`), `src/main.rs` (sostituire il segnaposto)
- Test: unit test in fondo a `src/cli.rs` e `src/run.rs`, più `tests/import.rs`

**Interfaces:**
- Consumes: `naming::{slug_from_path, resolve_collision}`, `probe::probe`, `catalog::{Catalog, default_dest, wallpaper_roots}`, `package::{PackageSpec, WriteError, sha256_file, sweep_stale_tmp, write}`, `ui::{Ui, KdeUi, SilentUi}`, `apply::{WallpaperSetter, PlasmaSetter}`
- Produces:
  - `cli::Cli` (clap `Parser`) con campi `apply: bool`, `fill_mode: Option<String>`, `dest: Option<PathBuf>`, `no_ui: bool`, `force: bool`, `min_size: (u32, u32)`, `files: Vec<PathBuf>`
  - `cli::parse_size(s: &str) -> Result<(u32, u32), String>`
  - `run::Outcome` con varianti `Imported { name: String, path: PathBuf }`, `Duplicate { of: String }`, `Cancelled`, `Failed { message: String }`
  - `run::run(cli: &Cli, ui: &dyn Ui, setter: &dyn WallpaperSetter) -> i32`
  - `run::main() -> i32`
  - `run::summary_line(outcomes: &[Outcome]) -> String`
  - costanti `run::{EXIT_OK, EXIT_PARTIAL, EXIT_FAILED, EXIT_USAGE}`

- [ ] **Step 1: Scrivere i test della CLI (falliranno)**

`src/cli.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parse_size_accetta_wxh() {
        assert_eq!(parse_size("1920x1080"), Ok((1920, 1080)));
        assert_eq!(parse_size("800X600"), Ok((800, 600)));
    }

    #[test]
    fn parse_size_rifiuta_input_malformati() {
        for cattivo in ["1920", "axb", "1920x", "x1080", ""] {
            assert!(parse_size(cattivo).is_err(), "{cattivo:?} doveva fallire");
        }
    }

    #[test]
    fn i_default_rispettano_la_spec() {
        let cli = Cli::parse_from(["kde-wallpaper-import", "foto.png"]);
        assert_eq!(cli.min_size, (1024, 768));
        assert!(!cli.apply && !cli.no_ui && !cli.force);
        assert_eq!(cli.dest, None);
        assert_eq!(cli.files.len(), 1);
    }

    #[test]
    fn accetta_piu_file_e_le_opzioni() {
        let cli = Cli::parse_from([
            "kde-wallpaper-import",
            "--apply",
            "--fill-mode",
            "stretch",
            "--min-size",
            "640x480",
            "a.png",
            "b.jpg",
        ]);
        assert!(cli.apply);
        assert_eq!(cli.fill_mode.as_deref(), Some("stretch"));
        assert_eq!(cli.min_size, (640, 480));
        assert_eq!(cli.files.len(), 2);
    }

    #[test]
    fn senza_file_e_errore_duso() {
        assert!(Cli::try_parse_from(["kde-wallpaper-import"]).is_err());
    }
}
```

- [ ] **Step 2: Verificare che i test falliscano**

Run: `cargo test cli`
Expected: FAIL in compilazione — `cannot find type Cli`.

- [ ] **Step 3: Implementare `cli`**

In testa a `src/cli.rs`:

```rust
use std::path::PathBuf;

use clap::Parser;

#[derive(Parser, Debug)]
#[command(
    name = "kde-wallpaper-import",
    about = "Importa immagini come pacchetti wallpaper KDE"
)]
pub struct Cli {
    /// Imposta come sfondo l'ultimo pacchetto importato
    #[arg(long)]
    pub apply: bool,

    /// Modalità di riempimento passata a plasma-apply-wallpaperimage
    #[arg(long, value_name = "MODE")]
    pub fill_mode: Option<String>,

    /// Root di destinazione (default: ${XDG_DATA_HOME:-$HOME/.local/share}/wallpapers)
    #[arg(long, value_name = "DIR")]
    pub dest: Option<PathBuf>,

    /// Nessun dialogo né notifica: solo stdout/stderr ed exit code
    #[arg(long)]
    pub no_ui: bool,

    /// Salta le conferme sulle immagini piccole
    #[arg(long)]
    pub force: bool,

    /// Soglia sotto la quale chiedere conferma
    #[arg(long, value_name = "WxH", default_value = "1024x768", value_parser = parse_size)]
    pub min_size: (u32, u32),

    #[arg(required = true, value_name = "FILE")]
    pub files: Vec<PathBuf>,
}

/// Interpreta `WxH`. Restituisce `String` perché è ciò che clap si aspetta da
/// un `value_parser`.
pub fn parse_size(s: &str) -> Result<(u32, u32), String> {
    let (w, h) = s
        .split_once(['x', 'X'])
        .ok_or_else(|| format!("formato atteso WxH, ricevuto «{s}»"))?;
    let w = w
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("larghezza non valida in «{s}»"))?;
    let h = h
        .trim()
        .parse::<u32>()
        .map_err(|_| format!("altezza non valida in «{s}»"))?;
    Ok((w, h))
}
```

Aggiungere `pub mod cli;` a `src/lib.rs`.

- [ ] **Step 4: Verificare che i test della CLI passino**

Run: `cargo test cli`
Expected: PASS, 5 test.

- [ ] **Step 5: Commit intermedio**

```bash
git add src/cli.rs src/lib.rs
git commit -m "feat(cli): superficie a riga di comando con parsing di --min-size"
```

- [ ] **Step 6: Scrivere i test di `run` (falliranno)**

`src/run.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::apply::testing::FakeSetter;
    use crate::ui::testing::FakeUi;
    use image::RgbImage;
    use tempfile::tempdir;

    fn immagine(dir: &Path, nome: &str, w: u32, h: u32, tinta: [u8; 3]) -> PathBuf {
        let path = dir.join(nome);
        let mut img = RgbImage::new(w, h);
        for px in img.pixels_mut() {
            *px = image::Rgb(tinta);
        }
        img.save(&path).unwrap();
        path
    }

    fn cli(dest: &Path, files: Vec<PathBuf>) -> Cli {
        Cli {
            apply: false,
            fill_mode: None,
            dest: Some(dest.to_path_buf()),
            no_ui: true,
            force: false,
            min_size: (1024, 768),
            files,
        }
    }

    #[test]
    fn immagine_piccola_chiede_conferma_e_puo_essere_annullata() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("wallpapers");
        let piccola = immagine(tmp.path(), "icona.png", 64, 64, [1, 2, 3]);

        let ui = FakeUi::new(false);
        let setter = FakeSetter::new();
        let code = run(&cli(&dest, vec![piccola]), &ui, &setter);

        assert_eq!(code, EXIT_OK, "annullare non è un errore");
        assert_eq!(ui.confirms().len(), 1);
        assert!(!dest.join("Icona").exists());
        assert!(!dest.join("icona").exists());
    }

    #[test]
    fn force_salta_la_conferma() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("wallpapers");
        let piccola = immagine(tmp.path(), "icona.png", 64, 64, [1, 2, 3]);

        let mut c = cli(&dest, vec![piccola]);
        c.force = true;
        let ui = FakeUi::new(false);
        let setter = FakeSetter::new();

        assert_eq!(run(&c, &ui, &setter), EXIT_OK);
        assert!(ui.confirms().is_empty());
        assert!(dest.join("icona/metadata.json").is_file());
    }

    #[test]
    fn apply_usa_lultimo_pacchetto_importato() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("wallpapers");
        let a = immagine(tmp.path(), "a.png", 1600, 900, [1, 0, 0]);
        let b = immagine(tmp.path(), "b.png", 1600, 900, [0, 1, 0]);

        let mut c = cli(&dest, vec![a, b]);
        c.apply = true;
        c.fill_mode = Some("stretch".to_string());
        let ui = FakeUi::new(true);
        let setter = FakeSetter::new();

        assert_eq!(run(&c, &ui, &setter), EXIT_OK);
        assert_eq!(
            setter.calls(),
            vec![(dest.join("b"), Some("stretch".to_string()))]
        );
    }

    #[test]
    fn apply_fallita_degrada_a_successo_parziale() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("wallpapers");
        let a = immagine(tmp.path(), "a.png", 1600, 900, [1, 0, 0]);

        let mut c = cli(&dest, vec![a]);
        c.apply = true;
        let ui = FakeUi::new(true);
        let setter = FakeSetter::failing();

        assert_eq!(run(&c, &ui, &setter), EXIT_PARTIAL);
        assert!(dest.join("a/metadata.json").is_file(), "l'import resta valido");
    }

    #[test]
    fn summary_line_omette_le_categorie_vuote() {
        let solo_import = [Outcome::Imported {
            name: "a".into(),
            path: PathBuf::from("/x/a"),
        }];
        assert_eq!(summary_line(&solo_import), "1 importati");

        let misto = [
            Outcome::Imported { name: "a".into(), path: PathBuf::from("/x/a") },
            Outcome::Duplicate { of: "b".into() },
            Outcome::Cancelled,
            Outcome::Failed { message: "boom".into() },
        ];
        assert_eq!(
            summary_line(&misto),
            "1 importati, 1 duplicati, 1 annullati, 1 errori"
        );
    }
}
```

- [ ] **Step 7: Verificare che i test falliscano**

Run: `cargo test run`
Expected: FAIL in compilazione — `cannot find function run`.

- [ ] **Step 8: Implementare `run`**

In testa a `src/run.rs`:

```rust
use std::path::{Path, PathBuf};
use std::time::Duration;

use clap::Parser;

use crate::apply::{PlasmaSetter, WallpaperSetter};
use crate::catalog::{self, Catalog};
use crate::cli::Cli;
use crate::naming;
use crate::package::{self, PackageSpec, WriteError};
use crate::probe;
use crate::ui::{KdeUi, SilentUi, Ui};

pub const EXIT_OK: i32 = 0;
pub const EXIT_PARTIAL: i32 = 1;
pub const EXIT_FAILED: i32 = 2;
pub const EXIT_USAGE: i32 = 64;

/// Età oltre la quale una tmp dir orfana viene rimossa.
const TMP_MAX_AGE: Duration = Duration::from_secs(24 * 3600);
/// Tentativi di rename prima di arrendersi su un nome.
const MAX_TENTATIVI_RENAME: usize = 5;

const TITOLO: &str = "Importa come sfondo";

#[derive(Debug)]
pub enum Outcome {
    Imported { name: String, path: PathBuf },
    Duplicate { of: String },
    Cancelled,
    Failed { message: String },
}

/// Punto di ingresso reale: monta le implementazioni concrete e delega a [`run`].
pub fn main() -> i32 {
    let cli = match Cli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let code = if e.use_stderr() { EXIT_USAGE } else { EXIT_OK };
            let _ = e.print();
            return code;
        }
    };

    let kde = KdeUi;
    let silent = SilentUi;
    let ui: &dyn Ui = if cli.no_ui { &silent } else { &kde };
    let setter = PlasmaSetter;

    run(&cli, ui, &setter)
}

/// Importa ogni file, riassume l'esito e restituisce l'exit code.
pub fn run(cli: &Cli, ui: &dyn Ui, setter: &dyn WallpaperSetter) -> i32 {
    let dest = cli.dest.clone().unwrap_or_else(catalog::default_dest);
    package::sweep_stale_tmp(&dest, TMP_MAX_AGE);

    let roots = catalog::wallpaper_roots(&dest);
    let mut cat = Catalog::scan(&dest, &roots);

    let mut outcomes = Vec::with_capacity(cli.files.len());
    let mut ultimo_importato: Option<PathBuf> = None;

    for file in &cli.files {
        let esito = importa_uno(file, &dest, &mut cat, cli, ui);
        match &esito {
            Outcome::Imported { name, path } => {
                println!("importato: {name}");
                ultimo_importato = Some(path.clone());
            }
            Outcome::Duplicate { of } => println!("duplicato di: {of}"),
            Outcome::Cancelled => println!("annullato: {}", file.display()),
            Outcome::Failed { message } => eprintln!("errore: {message}"),
        }
        outcomes.push(esito);
    }

    let falliti = outcomes
        .iter()
        .filter(|o| matches!(o, Outcome::Failed { .. }))
        .count();
    let riusciti = outcomes.len() - falliti;

    let mut code = if falliti == 0 {
        EXIT_OK
    } else if riusciti > 0 {
        EXIT_PARTIAL
    } else {
        EXIT_FAILED
    };

    let riepilogo = summary_line(&outcomes);
    println!("{riepilogo}");
    ui.notify("Importa come sfondo", &riepilogo, falliti > 0);

    if cli.apply {
        if let Some(path) = &ultimo_importato {
            if let Err(e) = setter.apply(path, cli.fill_mode.as_deref()) {
                eprintln!("errore: impossibile impostare lo sfondo: {e}");
                ui.notify(
                    "Importa come sfondo",
                    &format!("Impossibile impostare lo sfondo: {e}"),
                    true,
                );
                code = code.max(EXIT_PARTIAL);
            }
        }
    }

    code
}

fn importa_uno(
    file: &Path,
    dest: &Path,
    cat: &mut Catalog,
    cli: &Cli,
    ui: &dyn Ui,
) -> Outcome {
    let info = match probe::probe(file) {
        Ok(i) => i,
        Err(e) => {
            return Outcome::Failed {
                message: format!("{}: {e}", file.display()),
            }
        }
    };

    let (min_w, min_h) = cli.min_size;
    if !cli.force && (info.width < min_w || info.height < min_h) {
        let messaggio = format!(
            "«{}» è {}×{}, sotto la soglia di {min_w}×{min_h}. Importare comunque?",
            file.display(),
            info.width,
            info.height
        );
        if !ui.confirm(TITOLO, &messaggio) {
            return Outcome::Cancelled;
        }
    }

    let sha = match package::sha256_file(file) {
        Ok(s) => s,
        Err(e) => {
            return Outcome::Failed {
                message: format!("{}: impossibile calcolare l'hash ({e})", file.display()),
            }
        }
    };

    if let Some(esistente) = cat.duplicate_of(&sha) {
        return Outcome::Duplicate {
            of: esistente.to_string(),
        };
    }

    let base = naming::slug_from_path(file);

    for _ in 0..MAX_TENTATIVI_RENAME {
        let Some(nome) = naming::resolve_collision(&base, cat.taken()) else {
            return Outcome::Failed {
                message: format!(
                    "{}: troppi wallpaper già chiamati «{base}»",
                    file.display()
                ),
            };
        };
        let target = dest.join(&nome);
        let spec = PackageSpec {
            source: file,
            name: &nome,
            info: &info,
            sha256: &sha,
        };
        match package::write(&spec, &target) {
            Ok(()) => {
                cat.record_import(&nome, &sha);
                return Outcome::Imported { name: nome, path: target };
            }
            // Qualcun altro ha occupato il nome tra la risoluzione e il rename:
            // segnalo il nome come preso e riprovo col suffisso successivo.
            Err(WriteError::NameTaken) => cat.mark_taken(&nome),
            Err(WriteError::Other(e)) => {
                return Outcome::Failed {
                    message: format!("{}: {e}", file.display()),
                }
            }
        }
    }

    Outcome::Failed {
        message: format!("{}: impossibile trovare un nome libero", file.display()),
    }
}

/// Riga di riepilogo: gli importati ci sono sempre, le altre categorie solo
/// se non nulle.
pub fn summary_line(outcomes: &[Outcome]) -> String {
    let conta = |f: fn(&Outcome) -> bool| outcomes.iter().filter(|o| f(o)).count();

    let importati = conta(|o| matches!(o, Outcome::Imported { .. }));
    let duplicati = conta(|o| matches!(o, Outcome::Duplicate { .. }));
    let annullati = conta(|o| matches!(o, Outcome::Cancelled));
    let errori = conta(|o| matches!(o, Outcome::Failed { .. }));

    let mut parti = vec![format!("{importati} importati")];
    if duplicati > 0 {
        parti.push(format!("{duplicati} duplicati"));
    }
    if annullati > 0 {
        parti.push(format!("{annullati} annullati"));
    }
    if errori > 0 {
        parti.push(format!("{errori} errori"));
    }
    parti.join(", ")
}
```

Aggiungere `pub mod run;` a `src/lib.rs`.

Sostituire `src/main.rs`:

```rust
fn main() {
    std::process::exit(kde_wallpaper_importer::run::main());
}
```

- [ ] **Step 9: Verificare che i test di `run` passino**

Run: `cargo test run`
Expected: PASS, 5 test.

- [ ] **Step 10: Scrivere gli helper dei test di integrazione**

`tests/common/mod.rs`:

```rust
use std::path::{Path, PathBuf};

use assert_cmd::assert::Assert;
use assert_cmd::Command;
use image::RgbImage;

/// Scrive un PNG di tinta unita. Tinte diverse producono hash diversi.
pub fn immagine(dir: &Path, nome: &str, w: u32, h: u32, tinta: [u8; 3]) -> PathBuf {
    let path = dir.join(nome);
    let mut img = RgbImage::new(w, h);
    for px in img.pixels_mut() {
        *px = image::Rgb(tinta);
    }
    img.save(&path).expect("salvataggio immagine di prova");
    path
}

/// Crea un pacchetto wallpaper finto, per simulare quelli di sistema.
pub fn pacchetto_finto(root: &Path, nome: &str) {
    let dir = root.join(nome);
    std::fs::create_dir_all(dir.join("contents/images")).unwrap();
    std::fs::write(
        dir.join("metadata.json"),
        format!(r#"{{"KPlugin":{{"Id":"{nome}"}}}}"#),
    )
    .unwrap();
}

/// Invoca il binario reale con `--no-ui`, isolando le root XDG sulla tmpdir.
///
/// Restituisce un [`Assert`] su cui incatenare `.code(..)`, `.stdout(..)`,
/// `.stderr(..)` con i predicati di `predicates`.
pub fn importa(dest: &Path, xdg_data_dirs: &Path, args: &[&str]) -> Assert {
    let mut cmd = Command::cargo_bin("kde-wallpaper-import").expect("binario compilato");
    cmd.env("XDG_DATA_HOME", dest.parent().unwrap())
        .env("XDG_DATA_DIRS", xdg_data_dirs)
        .env("HOME", dest.parent().unwrap())
        .arg("--no-ui")
        .arg("--dest")
        .arg(dest);
    for a in args {
        cmd.arg(a);
    }
    cmd.assert()
}

/// Nessuna directory temporanea deve sopravvivere a un'esecuzione.
pub fn assert_niente_tmp(dest: &Path) {
    let Ok(entries) = std::fs::read_dir(dest) else {
        return;
    };
    let residui: Vec<_> = entries
        .flatten()
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with(".kwi-tmp-"))
        .collect();
    assert!(residui.is_empty(), "tmp dir residue: {residui:?}");
}
```

- [ ] **Step 11: Scrivere i test di integrazione**

`tests/import.rs`:

```rust
mod common;

use common::{assert_niente_tmp, immagine, importa, pacchetto_finto};
use predicates::prelude::*;
use tempfile::tempdir;

#[test]
fn import_base_produce_un_kpackage_valido() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();
    let src = immagine(tmp.path(), "Tramonto Cala Luna.png", 1600, 900, [10, 20, 30]);

    importa(&dest, &sistema, &[src.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: Tramonto_Cala_Luna"))
        .stdout(predicate::str::contains("1 importati"))
        .stderr(predicate::str::is_empty());

    let pkg = dest.join("Tramonto_Cala_Luna");
    assert!(pkg.join("metadata.json").is_file());
    assert!(pkg.join("contents/screenshot.png").is_file());
    let originale = pkg.join("contents/images/1600x900.png");
    assert_eq!(
        std::fs::read(&originale).unwrap(),
        std::fs::read(&src).unwrap()
    );

    let meta: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(pkg.join("metadata.json")).unwrap()).unwrap();
    assert_eq!(meta["KPlugin"]["Id"], "Tramonto_Cala_Luna");
    assert_eq!(meta["KPlugin"]["Name"], "Tramonto Cala Luna");

    assert_niente_tmp(&dest);
}

#[test]
fn stesso_file_due_volte_non_duplica() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();
    let src = immagine(tmp.path(), "foresta.png", 1600, 900, [1, 2, 3]);

    importa(&dest, &sistema, &[src.to_str().unwrap()]).success();

    importa(&dest, &sistema, &[src.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("duplicato di: foresta"))
        .stdout(predicate::str::contains("0 importati, 1 duplicati"));

    assert!(!dest.join("foresta-2").exists());
    assert_niente_tmp(&dest);
}

#[test]
fn file_diverso_con_lo_stesso_nome_prende_il_suffisso() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();

    let a = immagine(tmp.path(), "foresta.png", 1600, 900, [1, 2, 3]);
    let sub = tmp.path().join("altra");
    std::fs::create_dir_all(&sub).unwrap();
    let b = immagine(&sub, "foresta.png", 1600, 900, [9, 9, 9]);

    importa(&dest, &sistema, &[a.to_str().unwrap()]).success();
    importa(&dest, &sistema, &[b.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: foresta-2"));

    assert!(dest.join("foresta").is_dir());
    assert!(dest.join("foresta-2").is_dir());
    assert_niente_tmp(&dest);
}

#[test]
fn non_maschera_un_wallpaper_di_sistema() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(sistema.join("wallpapers")).unwrap();
    pacchetto_finto(&sistema.join("wallpapers"), "Altai");

    let src = immagine(tmp.path(), "Altai.png", 1600, 900, [4, 5, 6]);
    importa(&dest, &sistema, &[src.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: Altai-2"));

    assert!(dest.join("Altai-2").is_dir());
    assert!(!dest.join("Altai").exists(), "il nome di sistema resta libero");
    assert_niente_tmp(&dest);
}

#[test]
fn la_catena_dei_suffissi_riusa_i_buchi() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();

    for (i, tinta) in [[1, 1, 1], [2, 2, 2], [3, 3, 3]].iter().enumerate() {
        let sub = tmp.path().join(format!("d{i}"));
        std::fs::create_dir_all(&sub).unwrap();
        let f = immagine(&sub, "mare.png", 1600, 900, *tinta);
        importa(&dest, &sistema, &[f.to_str().unwrap()]).success();
    }
    assert!(dest.join("mare-3").is_dir());

    std::fs::remove_dir_all(dest.join("mare-2")).unwrap();
    let sub = tmp.path().join("d9");
    std::fs::create_dir_all(&sub).unwrap();
    let f = immagine(&sub, "mare.png", 1600, 900, [7, 7, 7]);
    importa(&dest, &sistema, &[f.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: mare-2"));

    assert!(dest.join("mare-2").is_dir(), "il buco viene riusato");
    assert_niente_tmp(&dest);
}

#[test]
fn file_corrotto_e_illeggibile_falliscono_senza_pacchetti_parziali() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();

    let corrotto = tmp.path().join("rotto.png");
    let buono = immagine(tmp.path(), "buono.png", 1600, 900, [1, 2, 3]);
    let bytes = std::fs::read(&buono).unwrap();
    std::fs::write(&corrotto, &bytes[..12]).unwrap();

    let assente = tmp.path().join("mai-esistito.png");

    importa(
        &dest,
        &sistema,
        &[corrotto.to_str().unwrap(), assente.to_str().unwrap()],
    )
    .code(2)
    .stdout(predicate::str::contains("0 importati, 2 errori"))
    .stderr(predicate::str::contains("danneggiata"))
    .stderr(predicate::str::contains("impossibile leggere il file"));

    assert!(!dest.join("rotto").exists());
    assert_niente_tmp(&dest);
}

#[test]
fn selezione_mista_restituisce_successo_parziale() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();

    let buono = immagine(tmp.path(), "buono.png", 1600, 900, [1, 2, 3]);
    importa(&dest, &sistema, &[buono.to_str().unwrap()]).success();

    let corrotto = tmp.path().join("rotto.png");
    std::fs::write(&corrotto, b"non sono un png").unwrap();
    let altro = immagine(tmp.path(), "altro.png", 1600, 900, [4, 4, 4]);

    importa(
        &dest,
        &sistema,
        &[
            altro.to_str().unwrap(),
            buono.to_str().unwrap(),
            corrotto.to_str().unwrap(),
        ],
    )
    .code(1)
    .stdout(predicate::str::contains("1 importati, 1 duplicati, 1 errori"));

    assert_niente_tmp(&dest);
}

#[test]
fn immagine_piccola_passa_con_force() {
    let tmp = tempdir().unwrap();
    let dest = tmp.path().join("share/wallpapers");
    let sistema = tmp.path().join("sistema");
    std::fs::create_dir_all(&sistema).unwrap();
    let piccola = immagine(tmp.path(), "icona.png", 64, 64, [1, 2, 3]);

    importa(&dest, &sistema, &["--force", piccola.to_str().unwrap()])
        .code(0)
        .stdout(predicate::str::contains("importato: icona"));

    assert!(dest.join("icona").is_dir());
    assert_niente_tmp(&dest);
}

#[test]
fn argomenti_mancanti_danno_exit_64() {
    assert_cmd::Command::cargo_bin("kde-wallpaper-import")
        .unwrap()
        .assert()
        .code(64)
        .stderr(predicate::str::contains("FILE"));
}
```

- [ ] **Step 12: Verificare che i test di integrazione passino**

Run: `cargo test --test import`
Expected: PASS, 9 test.

- [ ] **Step 13: Verificare fmt, clippy e suite completa**

Run: `cargo fmt --check && cargo clippy --all-targets -- -D warnings && cargo test`
Expected: tutto verde.

- [ ] **Step 14: Commit**

```bash
git add src/run.rs src/main.rs src/lib.rs tests/
git commit -m "feat(run): orchestrazione dell'import ed exit code

Dedup per hash prima del naming, retry sul rename quando il nome viene
occupato durante la corsa, riepilogo unico invece di una notifica per file.
Test end-to-end sul binario reale con root XDG isolate."
```

---

### Task 7: Servicemenu, Makefile e README

**Files:**
- Create: `share/kio/servicemenus/kde-wallpaper-importer.desktop.in`, `Makefile`, `README.md`

**Interfaces:**
- Consumes: il binario `kde-wallpaper-import` prodotto dalla Task 6
- Produces: target `build`, `test`, `install`, `uninstall`, `clean`

- [ ] **Step 1: Creare il template del servicemenu**

`share/kio/servicemenus/kde-wallpaper-importer.desktop.in`:

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

- [ ] **Step 2: Creare il Makefile**

`Makefile`:

```make
PREFIX  ?= $(HOME)/.local
BINDIR  := $(PREFIX)/bin
MENUDIR := $(PREFIX)/share/kio/servicemenus
BIN     := kde-wallpaper-import
MENU    := kde-wallpaper-importer.desktop

.PHONY: build test install uninstall clean

build:
	cargo build --release

test:
	cargo test

install: build
	install -Dm755 target/release/$(BIN) "$(BINDIR)/$(BIN)"
	install -d "$(MENUDIR)"
	sed 's|@BINARY@|$(BINDIR)/$(BIN)|g' share/kio/servicemenus/$(MENU).in > "$(MENUDIR)/$(MENU)"
	chmod 644 "$(MENUDIR)/$(MENU)"
	-kbuildsycoca6 --noincremental >/dev/null 2>&1

uninstall:
	rm -f "$(BINDIR)/$(BIN)" "$(MENUDIR)/$(MENU)"
	-kbuildsycoca6 --noincremental >/dev/null 2>&1

clean:
	cargo clean
```

- [ ] **Step 3: Verificare l'installazione in un prefisso temporaneo**

```bash
PREFIX_TEST=$(mktemp -d)
make install PREFIX="$PREFIX_TEST"
test -x "$PREFIX_TEST/bin/kde-wallpaper-import" || echo "FALLITO: binario mancante"
grep -q "^Exec=$PREFIX_TEST/bin/kde-wallpaper-import %F$" \
  "$PREFIX_TEST/share/kio/servicemenus/kde-wallpaper-importer.desktop" \
  || echo "FALLITO: @BINARY@ non sostituito"
grep -q "^Exec=$PREFIX_TEST/bin/kde-wallpaper-import --apply %F$" \
  "$PREFIX_TEST/share/kio/servicemenus/kde-wallpaper-importer.desktop" \
  || echo "FALLITO: azione --apply non sostituita"
stat -c '%a' "$PREFIX_TEST/share/kio/servicemenus/kde-wallpaper-importer.desktop"
make uninstall PREFIX="$PREFIX_TEST"
test ! -e "$PREFIX_TEST/bin/kde-wallpaper-import" || echo "FALLITO: uninstall incompleto"
rm -rf "$PREFIX_TEST"
```

Expected: nessuna riga «FALLITO», `stat` stampa `644`.

- [ ] **Step 4: Scrivere il README**

`README.md`:

````markdown
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
````

- [ ] **Step 5: Commit**

```bash
git add Makefile README.md share/
git commit -m "feat(install): servicemenu, Makefile e documentazione

Il percorso assoluto del binario viene sostituito nell'Exec a install time:
Dolphin non eredita necessariamente ~/.local/bin nel PATH."
```

---

### Task 8: Verifica manuale su Dolphin

L'unica parte che i test automatici non possono coprire: che KDE veda davvero il servicemenu e il pacchetto.

**Files:** nessuno (verifica)

**Interfaces:**
- Consumes: tutto quanto sopra
- Produces: nessun artefatto

- [ ] **Step 1: Installare per l'utente corrente**

```bash
make install
```

- [ ] **Step 2: Verificare la comparsa delle voci**

Aprire Dolphin (riavviarlo se era già aperto), tasto destro su un file `.jpg`
o `.png`. Attese: **Importa come sfondo** e **Importa e imposta come sfondo**.

Tasto destro su un file non immagine (es. un `.txt`): le voci **non** devono
comparire.

- [ ] **Step 3: Importare un'immagine e verificarla nel selettore**

Usare «Importa come sfondo» su una foto. Attesa: notifica «1 importati».

```bash
ls ~/.local/share/wallpapers/
```

Aprire *Impostazioni di sistema → Sfondo* (o tasto destro sul desktop →
Configura). Attesa: il nuovo wallpaper compare nell'elenco con l'anteprima.

- [ ] **Step 4: Verificare la collisione con un nome di sistema**

```bash
cp ~/Immagini/una-foto.jpg /tmp/Altai.jpg   # Altai esiste in /usr/share/wallpapers
kde-wallpaper-import /tmp/Altai.jpg
ls -d ~/.local/share/wallpapers/Altai*
```

Attesa: viene creata `Altai-2`, non `Altai`. Nel selettore di Plasma devono
comparire **entrambi** i wallpaper.

- [ ] **Step 5: Verificare il dedup**

```bash
kde-wallpaper-import /tmp/Altai.jpg
```

Attesa: `duplicato di: Altai-2`, nessuna nuova directory.

- [ ] **Step 6: Verificare «Importa e imposta come sfondo»**

Tasto destro su un'altra immagine → «Importa e imposta come sfondo». Attesa: lo
sfondo del desktop cambia.

- [ ] **Step 7: Verificare la selezione multipla**

Selezionare 3 immagini insieme, tasto destro → «Importa come sfondo». Attesa:
notifica «3 importati», tre nuove directory.

- [ ] **Step 8: Verificare la disinstallazione**

```bash
make uninstall
```

Attesa: le voci spariscono dal menù di Dolphin (riavviarlo), i wallpaper
importati restano nel selettore di Plasma.

- [ ] **Step 9: Annotare l'esito**

Se qualcosa non si comporta come atteso, aprire un'issue o annotarlo prima di
considerare il lavoro concluso. Non ci sono commit in questa task.

---

## Note di self-review

Copertura della spec verificata sezione per sezione: architettura (Task 1–6),
componenti (Task 1–6, uno per modulo), servicemenu (Task 7), flusso dati
(Task 6), struttura del pacchetto e `metadata.json` (Task 4), naming e
collisioni (Task 1 + Task 3), scrittura atomica (Task 4), gestione errori ed
exit code (Task 2 + Task 6), test (in ogni task), installazione (Task 7).

Tre scostamenti consapevoli dalla spec, tutti documentati sopra:

1. `src/lib.rs`, `src/cli.rs` e `src/run.rs` non erano nell'elenco file della
   spec. Servono perché un crate solo-bin non è raggiungibile dai test.
2. `Catalog` espone `mark_taken` e `record_import` invece di un unico
   `insert`: il retry sul rename deve occupare un nome senza inquinare la
   mappa degli hash con una stringa vuota.
3. Il riepilogo distingue anche gli «annullati», categoria implicita nella
   spec (la conferma negata non è né import né errore) ma non enumerata.

Due emendamenti decisi nella scansione pre-volo, prima di iniziare
l'esecuzione:

4. I test di integrazione usano davvero i combinatori di `predicates` invece
   di frugare a mano in `Output`: la dev-dependency era dichiarata ma
   inutilizzata, e gli assert sull'output diventano più espliciti.
5. `apply.rs` espone `plasma_args`, funzione pura testabile, così il modulo
   non resta completamente scoperto. Lo spawn del processo resta non
   testato: verificarlo cambierebbe lo sfondo della macchina di test.
