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
