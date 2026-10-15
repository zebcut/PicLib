//! Réglages, bilan et erreurs d'un import.

use std::collections::HashSet;
use std::path::PathBuf;

use super::copy::DestinationRefusal;
use crate::catalog::CatalogError;
use crate::catalog::repositories::metadata_templates::MetadataTemplate;
use crate::domain::{ImportCounts, ImportId};
use crate::files::{CopyError, ScanError, SourceScan};

/// Réglages d'un import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportOptions {
    /// Ne pas importer les doublons(activé par défaut).
    pub skip_duplicates: bool,
    /// Modèle de métadonnées posé sur chaque fichier ajouté, lu
    /// au lancement ; il fait partie de l'import (pas de Cmd+Z).
    pub template: Option<MetadataTemplate>,
    /// Nombre de fichiers écrits par transaction. Chaque lot est enregistré
    /// en entier ou pas du tout : une annulation ou une erreur laisse le
    /// catalogue cohérent, avec les lots déjà écrits.
    pub batch_size: usize,
    /// Empiler les paires RAW + JPEG importées (activé par défaut), en fin d'import.
    pub stack_pairs: bool,
    /// Empiler les rafales importées, avec cet écart maximal entre deux
    /// photos, en secondes ; `None` : non (par défaut).
    pub burst_gap_seconds: Option<u32>,
    /// Fichiers cochés dans l'écran d'import ; `None` : toutes
    /// les photos et vidéos de la source.
    pub only: Option<HashSet<PathBuf>>,
    /// Parcours de la source : sous-dossiers compris ou non, dossiers
    /// internes du catalogue exclus.
    pub scan: SourceScan,
}

impl Default for ImportOptions {
    fn default() -> Self {
        Self {
            skip_duplicates: true,
            template: None,
            batch_size: 500,
            stack_pairs: true,
            burst_gap_seconds: None,
            only: None,
            scan: SourceScan::default(),
        }
    }
}

/// Bilan d'un import terminé.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportReport {
    /// Session d'import enregistrée au catalogue.
    pub import_id: ImportId,
    /// Compteurs (ajoutés, déjà présents, doublons...).
    pub counts: ImportCounts,
    /// Nom du modèle de métadonnées posé sur les fichiers ajoutés.
    pub template_name: Option<String>,
    /// Éléments illisibles, non importés.
    pub unreadable: Vec<PathBuf>,
    /// Import par copie : dossier où les fichiers ont été rangés.
    pub destination: Option<PathBuf>,
    /// Import par copie : photos et vidéos qui n'ont pas pu être copiées
    /// (non ajoutées).
    pub copy_failed: Vec<PathBuf>,
    /// Import par copie : fichiers annexes (XMP, `.dop`) qui n'ont pas pu
    /// être copiés ; leur photo est ajoutée, avec leurs métadonnées.
    pub sidecars_not_copied: Vec<PathBuf>,
    /// Copie de sauvegarde : dossier où les originaux ont été copiés.
    pub backup_folder: Option<PathBuf>,
    /// Copie de sauvegarde : fichiers qui n'ont pas pu y être copiés (ils
    /// sont importés quand même).
    pub backup_failed: Vec<PathBuf>,
}

/// Erreur empêchant ou interrompant un import.
#[derive(Debug, thiserror::Error)]
pub enum ImportError {
    /// Le dossier est sur une carte mémoire ou un appareil photo : seul
    /// l'import par copie est permis.
    #[error("{} is on a camera card: reference import is not allowed", path.display())]
    CameraCard {
        /// Dossier choisi.
        path: PathBuf,
    },
    /// Le dossier est introuvable, illisible ou n'est pas un dossier.
    #[error(transparent)]
    Source(ScanError),
    /// La destination d'un import par copie est refusée : rien n'a été
    /// copié.
    #[error(transparent)]
    DestinationRefused(#[from] DestinationRefusal),
    /// La destination ne peut plus recevoir de copies (disque plein,
    /// droits...) ; les fichiers déjà copiés et vérifiés restent au
    /// catalogue.
    #[error(transparent)]
    Destination(CopyError),
    /// L'utilisateur a annulé ; les lots déjà écrits restent au catalogue.
    #[error("import cancelled")]
    Cancelled,
    /// Le catalogue n'a pas pu être lu ou écrit ; les lots déjà écrits
    /// restent au catalogue.
    #[error("catalog error during import: {0}")]
    Catalog(#[from] CatalogError),
}

impl From<ScanError> for ImportError {
    fn from(error: ScanError) -> Self {
        match error {
            ScanError::Cancelled => Self::Cancelled,
            other => Self::Source(other),
        }
    }
}
