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
    Imported {
        name: String,
        path: PathBuf,
    },
    /// `path` e' il pacchetto gia' presente: serve a `--apply`, che deve poter
    /// impostare come sfondo anche un'immagine gia' in libreria.
    Duplicate {
        of: String,
        path: PathBuf,
    },
    Cancelled,
    Failed {
        message: String,
    },
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
    // Ultimo pacchetto *elaborato*, non solo importato: un duplicato e' un
    // pacchetto valido gia' su disco, e `--apply` deve poterlo impostare.
    let mut ultimo_elaborato: Option<PathBuf> = None;

    for file in &cli.files {
        let esito = importa_uno(file, &dest, &mut cat, cli, ui);
        match &esito {
            Outcome::Imported { name, path } => {
                println!("importato: {name}");
                ultimo_elaborato = Some(path.clone());
            }
            Outcome::Duplicate { of, path } => {
                println!("duplicato di: {of}");
                ultimo_elaborato = Some(path.clone());
            }
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
        if let Some(path) = &ultimo_elaborato {
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

fn importa_uno(file: &Path, dest: &Path, cat: &mut Catalog, cli: &Cli, ui: &dyn Ui) -> Outcome {
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
        let of = esistente.to_string();
        return Outcome::Duplicate {
            path: dest.join(&of),
            of,
        };
    }

    let base = naming::slug_from_path(file);

    for _ in 0..MAX_TENTATIVI_RENAME {
        let Some(nome) = naming::resolve_collision(&base, cat.taken()) else {
            return Outcome::Failed {
                message: format!("{}: troppi wallpaper già chiamati «{base}»", file.display()),
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
                return Outcome::Imported {
                    name: nome,
                    path: target,
                };
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
        assert!(
            dest.join("a/metadata.json").is_file(),
            "l'import resta valido"
        );
    }

    #[test]
    fn apply_imposta_anche_un_pacchetto_duplicato() {
        // La voce di menu' «Importa e imposta come sfondo» deve funzionare
        // anche su un'immagine gia' in libreria: il pacchetto esiste, quindi
        // non c'e' ragione di non applicarlo.
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("wallpapers");
        let a = immagine(tmp.path(), "a.png", 1600, 900, [1, 0, 0]);

        let ui = FakeUi::new(true);
        let primo = FakeSetter::new();
        assert_eq!(run(&cli(&dest, vec![a.clone()]), &ui, &primo), EXIT_OK);
        assert!(
            primo.calls().is_empty(),
            "senza --apply non si applica nulla"
        );

        let mut c = cli(&dest, vec![a]);
        c.apply = true;
        let secondo = FakeSetter::new();
        assert_eq!(run(&c, &ui, &secondo), EXIT_OK);
        assert_eq!(
            secondo.calls(),
            vec![(dest.join("a"), None)],
            "il duplicato va comunque impostato come sfondo"
        );
    }

    #[test]
    fn summary_line_omette_le_categorie_vuote() {
        let solo_import = [Outcome::Imported {
            name: "a".into(),
            path: PathBuf::from("/x/a"),
        }];
        assert_eq!(summary_line(&solo_import), "1 importati");

        let misto = [
            Outcome::Imported {
                name: "a".into(),
                path: PathBuf::from("/x/a"),
            },
            Outcome::Duplicate {
                of: "b".into(),
                path: PathBuf::from("/x/b"),
            },
            Outcome::Cancelled,
            Outcome::Failed {
                message: "boom".into(),
            },
        ];
        assert_eq!(
            summary_line(&misto),
            "1 importati, 1 duplicati, 1 annullati, 1 errori"
        );
    }
}
