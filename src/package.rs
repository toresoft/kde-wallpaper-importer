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
    let img = image::open(source)?;
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
        ImageInfo {
            width: w,
            height: h,
            format: ImageFormat::Png,
        }
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
        assert!(
            v["KPlugin"]["Authors"].is_null(),
            "non attribuiamo l'immagine all'utente"
        );
    }

    #[test]
    fn write_crea_la_struttura_kpackage() {
        let dir = tempdir().unwrap();
        let src = immagine(dir.path(), "foto.png", 1600, 900);
        let dest_root = dir.path().join("wallpapers");
        let target = dest_root.join("Foto");

        let i = info(1600, 900);
        let spec = PackageSpec {
            source: &src,
            name: "Foto",
            info: &i,
            sha256: "abc",
        };
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
        let spec = PackageSpec {
            source: &grande,
            name: "Grande",
            info: &i,
            sha256: "a",
        };
        write(&spec, &dest_root.join("Grande")).unwrap();
        let s = image::open(dest_root.join("Grande/contents/screenshot.png")).unwrap();
        assert_eq!(s.width(), SCREENSHOT_MAX);
        assert_eq!(s.height(), 720);

        let piccola = immagine(dir.path(), "piccola.png", 300, 200);
        let i = info(300, 200);
        let spec = PackageSpec {
            source: &piccola,
            name: "Piccola",
            info: &i,
            sha256: "b",
        };
        write(&spec, &dest_root.join("Piccola")).unwrap();
        let s = image::open(dest_root.join("Piccola/contents/screenshot.png")).unwrap();
        assert_eq!((s.width(), s.height()), (300, 200), "niente upscaling");
    }

    #[test]
    fn write_su_nome_esistente_restituisce_name_taken() {
        let dir = tempdir().unwrap();
        let src = immagine(dir.path(), "foto.png", 100, 100);
        let dest_root = dir.path().join("wallpapers");
        let target = dest_root.join("Foto");
        std::fs::create_dir_all(&target).unwrap();

        let i = info(100, 100);
        let spec = PackageSpec {
            source: &src,
            name: "Foto",
            info: &i,
            sha256: "abc",
        };
        assert!(matches!(write(&spec, &target), Err(WriteError::NameTaken)));
    }

    #[test]
    fn un_fallimento_non_lascia_tmp_dir() {
        let dir = tempdir().unwrap();
        let dest_root = dir.path().join("wallpapers");
        std::fs::create_dir_all(&dest_root).unwrap();
        let assente = dir.path().join("assente.png");

        let i = info(100, 100);
        let spec = PackageSpec {
            source: &assente,
            name: "Foto",
            info: &i,
            sha256: "abc",
        };
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
        let estranea = root.join("Foresta");
        for d in [&vecchia, &recente, &estranea] {
            std::fs::create_dir_all(d).unwrap();
        }
        // Retrodata la mtime di `vecchia` di 48 ore.
        let due_giorni_fa = std::time::SystemTime::now() - Duration::from_secs(48 * 3600);
        filetime_set(&vecchia, due_giorni_fa);

        sweep_stale_tmp(root, Duration::from_secs(24 * 3600));

        assert!(!vecchia.exists());
        assert!(recente.exists());
        assert!(estranea.exists());
    }

    /// Imposta la mtime di una directory senza dipendenze aggiuntive.
    fn filetime_set(path: &Path, when: std::time::SystemTime) {
        use std::os::unix::ffi::OsStrExt;
        let secs = when.duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let times = [
            libc::timespec {
                tv_sec: secs,
                tv_nsec: 0,
            },
            libc::timespec {
                tv_sec: secs,
                tv_nsec: 0,
            },
        ];
        let c = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
        let rc = unsafe { libc::utimensat(libc::AT_FDCWD, c.as_ptr(), times.as_ptr(), 0) };
        assert_eq!(rc, 0, "utimensat fallita");
    }
}
