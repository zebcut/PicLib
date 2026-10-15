//! Formats de fichiers pris en charge, reconnus par leur extension.

use std::path::Path;

use super::MediaKind;

/// Format d'un fichier photo ou vidéo pris en charge par PicLib.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaFormat {
    /// JPEG (`.jpg`, `.jpeg`).
    Jpeg,
    /// PNG.
    Png,
    /// HEIC / HEIF, format des iPhone.
    Heic,
    /// TIFF.
    Tiff,
    /// Photoshop (`.psd`, `.psb`) : son image aplatie, lue par le système
    ///.
    Psd,
    /// RAW Canon (anciens boîtiers).
    Cr2,
    /// RAW Canon (boîtiers récents).
    Cr3,
    /// RAW Nikon.
    Nef,
    /// RAW Sony.
    Arw,
    /// RAW Fujifilm.
    Raf,
    /// RAW « universel » d'Adobe, aussi produit par certains téléphones.
    ///
    /// This product includes DNG technology under license by Adobe.
    Dng,
    /// Vidéo MP4.
    Mp4,
    /// Vidéo QuickTime.
    Mov,
    /// Vidéo MPEG-4 d'Apple.
    M4v,
    /// Vidéo AVI.
    Avi,
    /// Vidéo AVCHD des caméscopes (`.mts`, `.m2ts`).
    Mts,
    /// Vidéo 3GP des anciens téléphones.
    ThreeGp,
    /// Vidéo Matroska.
    Mkv,
}

impl MediaFormat {
    /// Tous les formats pris en charge.
    pub const ALL: [Self; 18] = [
        Self::Jpeg,
        Self::Png,
        Self::Heic,
        Self::Tiff,
        Self::Psd,
        Self::Cr2,
        Self::Cr3,
        Self::Nef,
        Self::Arw,
        Self::Raf,
        Self::Dng,
        Self::Mp4,
        Self::Mov,
        Self::M4v,
        Self::Avi,
        Self::Mts,
        Self::ThreeGp,
        Self::Mkv,
    ];

    /// Reconnaît le format d'après l'extension, sans tenir compte de la casse
    /// (`IMG_0001.JPG` et `img_0001.jpg` sont tous deux des JPEG). Renvoie
    /// `None` pour un fichier non pris en charge.
    #[must_use]
    pub fn from_extension(extension: &str) -> Option<Self> {
        let format = match extension.to_ascii_lowercase().as_str() {
            "jpg" | "jpeg" => Self::Jpeg,
            "png" => Self::Png,
            "heic" | "heif" => Self::Heic,
            "tif" | "tiff" => Self::Tiff,
            "psd" | "psb" => Self::Psd,
            "cr2" => Self::Cr2,
            "cr3" => Self::Cr3,
            "nef" => Self::Nef,
            "arw" => Self::Arw,
            "raf" => Self::Raf,
            "dng" => Self::Dng,
            "mp4" => Self::Mp4,
            "mov" => Self::Mov,
            "m4v" => Self::M4v,
            "avi" => Self::Avi,
            "mts" | "m2ts" => Self::Mts,
            "3gp" => Self::ThreeGp,
            "mkv" => Self::Mkv,
            _ => return None,
        };
        Some(format)
    }

    /// Reconnaît le format d'un chemin d'après son extension.
    #[must_use]
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(Self::from_extension)
    }

    /// Photo ou vidéo.
    #[must_use]
    pub fn kind(self) -> MediaKind {
        match self {
            Self::Mp4
            | Self::Mov
            | Self::M4v
            | Self::Avi
            | Self::Mts
            | Self::ThreeGp
            | Self::Mkv => MediaKind::Video,
            _ => MediaKind::Photo,
        }
    }

    /// Fichier RAW d'appareil photo.
    #[must_use]
    pub fn is_raw(self) -> bool {
        matches!(
            self,
            Self::Cr2 | Self::Cr3 | Self::Nef | Self::Arw | Self::Raf | Self::Dng
        )
    }
}
