//! Photo ou vidéo du catalogue.

use unicode_normalization::UnicodeNormalization;

use super::{CaptureTime, FolderId, ImportId, MediaDetails, MediaFormat, MediaId, Timestamp};

/// Nature d'un média.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MediaKind {
    /// Image fixe (JPEG, HEIC, RAW...).
    Photo,
    /// Vidéo (MP4, MOV...).
    Video,
}

/// Données d'un média à ajouter au catalogue (son identifiant sera attribué
/// par la base).
#[derive(Debug, Clone, PartialEq)]
pub struct NewMedia {
    /// Dossier qui contient le fichier.
    pub folder_id: FolderId,
    /// Nom actuel du fichier, extension comprise.
    pub file_name: String,
    /// Nom du fichier au moment de l'import : il reste identique si le
    /// fichier est renommé ensuite (détection des doublons).
    pub original_file_name: String,
    /// Format du fichier.
    pub format: MediaFormat,
    /// Taille du fichier en octets.
    pub file_size: u64,
    /// Date de dernière modification du fichier sur le disque.
    pub file_modified_at: Timestamp,
    /// Date de prise de vue.
    pub capture_time: CaptureTime,
    /// Informations techniques lues dans les métadonnées.
    pub details: MediaDetails,
    /// Import qui a ajouté ce média.
    pub import_id: ImportId,
    /// Empreinte SHA-256 du contenu (64 caractères hexadécimaux), connue
    /// pour les fichiers importés par copie(doublons).
    pub content_hash: Option<String>,
}

impl NewMedia {
    /// Photo ou vidéo, d'après le format.
    #[must_use]
    pub fn kind(&self) -> MediaKind {
        self.format.kind()
    }

    /// Clé du nom d'origine pour la détection des doublons.
    #[must_use]
    pub fn name_key(&self) -> String {
        duplicate_name_key(&self.original_file_name)
    }
}

/// Forme d'un nom de fichier utilisée pour reconnaître les doublons :
///
/// - normalisée en Unicode NFC, car « é » peut s'écrire en un caractère ou
///   en deux (« e » + accent) selon le système qui a créé le fichier ;
/// - en minuscules, car `IMG_0001.JPG` et `img_0001.jpg` désignent la même
///   photo pour un utilisateur (et pour le système de fichiers de macOS).
#[must_use]
pub fn duplicate_name_key(file_name: &str) -> String {
    file_name.nfc().collect::<String>().to_lowercase()
}

/// Média enregistré dans le catalogue.
#[derive(Debug, Clone, PartialEq)]
pub struct Media {
    /// Identifiant dans le catalogue.
    pub id: MediaId,
    /// Données du média.
    pub data: NewMedia,
}
