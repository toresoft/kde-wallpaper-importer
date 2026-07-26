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
            Self {
                fallisce: false,
                calls: RefCell::new(Vec::new()),
            }
        }

        pub fn failing() -> Self {
            Self {
                fallisce: true,
                calls: RefCell::new(Vec::new()),
            }
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
