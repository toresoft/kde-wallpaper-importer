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
            assert_eq!(
                slug_from_path(Path::new(input)),
                atteso,
                "slug_from_path({input:?})"
            );
        }
    }

    #[test]
    fn name_key_e_case_insensitive() {
        assert_eq!(name_key("Foresta"), name_key("foresta"));
    }

    #[test]
    fn resolve_collision_restituisce_il_base_se_libero() {
        let taken = HashSet::new();
        assert_eq!(
            resolve_collision("foresta", &taken).as_deref(),
            Some("foresta")
        );
    }

    #[test]
    fn resolve_collision_aggiunge_suffissi_progressivi() {
        let taken: HashSet<String> = ["foresta", "foresta-2"]
            .iter()
            .map(|s| name_key(s))
            .collect();
        assert_eq!(
            resolve_collision("foresta", &taken).as_deref(),
            Some("foresta-3")
        );
    }

    #[test]
    fn resolve_collision_ignora_le_maiuscole() {
        let taken: HashSet<String> = ["Foresta"].iter().map(|s| name_key(s)).collect();
        assert_eq!(
            resolve_collision("foresta", &taken).as_deref(),
            Some("foresta-2")
        );
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
