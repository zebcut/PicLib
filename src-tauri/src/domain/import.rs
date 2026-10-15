//! Session d'import.

use std::path::PathBuf;

use super::{ImportId, Timestamp};

/// Façon dont les fichiers entrent dans le catalogue.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportMode {
    /// Les fichiers restent où ils sont ; le catalogue pointe vers eux.
    Reference,
    /// Les fichiers sont copiés (et vérifiés) vers un dossier de destination.
    Copy,
}

/// État d'une session d'import.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ImportStatus {
    /// En cours (ou interrompu brutalement, application fermée).
    Running,
    /// Terminé normalement.
    Completed,
    /// Arrêté par l'utilisateur ; les fichiers déjà ajoutés restent.
    Cancelled,
    /// Arrêté sur une erreur ; les fichiers déjà ajoutés restent.
    Failed,
}

/// Compteurs d'une session d'import.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ImportCounts {
    /// Photos ajoutées au catalogue.
    pub added_photos: u64,
    /// Vidéos ajoutées au catalogue.
    pub added_videos: u64,
    /// Fichiers déjà présents au catalogue (même emplacement).
    pub already_in_catalog: u64,
    /// Doublons ignorés (même nom d'origine, même date, même taille).
    pub duplicates_skipped: u64,
    /// Fichiers ajoutés sans date de prise de vue dans leurs métadonnées
    /// (date du fichier utilisée).
    pub without_exif_date: u64,
    /// Fichiers ajoutés dont les métadonnées n'ont pas pu être lues (EXIF
    /// ou XMP, en tout ou en partie).
    pub unreadable_metadata: u64,
    /// Fichiers ajoutés avec des mots-clés trouvés dans leurs métadonnées.
    pub with_recovered_keywords: u64,
    /// Fichiers ajoutés dont les sources de métadonnées se contredisaient
    /// (la plus prioritaire l'a emporté).
    pub metadata_conflicts: u64,
    /// Import par copie : fichiers qui n'ont pas pu être copiés (source
    /// illisible, copie différente de la source), donc pas ajoutés.
    pub copy_failures: u64,
}

/// Une session d'import : un lot de fichiers ajoutés en une fois.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Import {
    /// Identifiant dans le catalogue.
    pub id: ImportId,
    /// Mode d'import utilisé.
    pub mode: ImportMode,
    /// Dossier ou volume d'origine des fichiers.
    pub source_path: PathBuf,
    /// Import par copie : dossier où les fichiers ont été rangés.
    pub destination_path: Option<PathBuf>,
    /// Début de l'import.
    pub started_at: Timestamp,
    /// Fin de l'import ; absente tant qu'il est en cours.
    pub finished_at: Option<Timestamp>,
    /// État.
    pub status: ImportStatus,
    /// Compteurs.
    pub counts: ImportCounts,
}
