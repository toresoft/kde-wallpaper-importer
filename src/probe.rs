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
        self.format
            .extensions_str()
            .first()
            .copied()
            .unwrap_or("img")
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

    Ok(ImageInfo {
        width,
        height,
        format,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageFormat, RgbImage};
    use tempfile::tempdir;

    fn scrivi_immagine(dir: &std::path::Path, nome: &str, w: u32, h: u32) -> std::path::PathBuf {
        let path = dir.join(nome);
        RgbImage::new(w, h)
            .save(&path)
            .expect("salvataggio immagine di prova");
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
