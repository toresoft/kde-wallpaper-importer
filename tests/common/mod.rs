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
