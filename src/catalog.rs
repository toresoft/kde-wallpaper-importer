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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn pacchetto(root: &Path, nome: &str, sha: Option<&str>) {
        let dir = root.join(nome);
        std::fs::create_dir_all(dir.join("contents/images")).unwrap();
        let json = match sha {
            Some(s) => {
                format!(r#"{{"KPlugin":{{"Id":"{nome}"}},"X-KWI":{{"SourceSha256":"{s}"}}}}"#)
            }
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
        let cat = Catalog::scan(&dest, std::slice::from_ref(&dest));
        assert!(cat.is_taken("Altai"));
        assert_eq!(cat.duplicate_of("aaa"), None);
    }

    #[test]
    fn record_import_e_mark_taken_aggiornano_lo_stato() {
        let tmp = tempdir().unwrap();
        let dest = tmp.path().join("utente");
        std::fs::create_dir_all(&dest).unwrap();
        let mut cat = Catalog::scan(&dest, std::slice::from_ref(&dest));

        cat.record_import("Mare", "ccc");
        assert!(cat.is_taken("mare"));
        assert_eq!(cat.duplicate_of("ccc"), Some("Mare"));

        cat.mark_taken("Cielo");
        assert!(cat.is_taken("cielo"));
        assert_eq!(
            cat.duplicate_of(""),
            None,
            "mark_taken non tocca la mappa hash"
        );
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
