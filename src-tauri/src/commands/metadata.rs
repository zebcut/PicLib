//! Panneau Métadonnées : valeurs de la sélection, informations de la photo
//! active, et modification annulable des champs descriptifs.

use std::sync::Mutex;

use serde::{Deserialize, Serialize};
use specta::Type;
use tauri::State;

use super::edits::{EditFailure, EditOutcome, failure, lock, media_ids_of, outcome};
use crate::catalog::Catalog;
use crate::catalog::repositories::metadata::{self, FieldSummary, MediaInfo};
use crate::catalog::repositories::{capture_time, edited_versions};
use crate::domain::{MAX_METADATA_CHARS, MediaId, MetadataField, metadata_text};
use crate::edits::{Edit, History};
use crate::external_editing;

/// Champ de métadonnées descriptives, tel qu'échangé avec l'interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum MetadataFieldDto {
    /// Titre.
    Title,
    /// Légende.
    Caption,
    /// Auteur.
    Creator,
    /// Copyright.
    Copyright,
    /// Pays.
    Country,
    /// Région.
    Region,
    /// Ville.
    City,
    /// Lieu précis.
    Location,
}

/// Valeur d'un champ pour les photos sélectionnées.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FieldValueDto {
    /// Toutes ont la même valeur (`null` : vide).
    Same {
        /// Valeur commune.
        value: Option<String>,
    },
    /// Les valeurs diffèrent.
    Mixed,
}

/// Un champ du panneau et sa valeur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct FieldState {
    /// Champ.
    pub field: MetadataFieldDto,
    /// Valeur pour les photos sélectionnées.
    pub value: FieldValueDto,
}

/// Position GPS en degrés décimaux.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct GpsDto {
    /// Latitude, de -90 à 90.
    pub latitude: f64,
    /// Longitude, de -180 à 180.
    pub longitude: f64,
    /// Altitude en mètres, si connue.
    pub altitude: Option<f64>,
}

/// Informations en lecture seule de la photo active.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaInfoDto {
    /// Nom du fichier.
    pub file_name: String,
    /// Dossier du fichier.
    pub folder: String,
    /// Taille du fichier en octets (nombre exact jusqu'à 8 Po côté
    /// interface).
    pub file_size: f64,
    /// Date de prise de vue lue à l'import (`2024-07-14T18:30:05`),
    /// seulement si la date a été modifiée depuis. La date actuelle est
    /// dans [`MetadataPanel::capture_time`].
    pub original_capture_time: Option<String>,
    /// Largeur en pixels.
    pub width: Option<u32>,
    /// Hauteur en pixels.
    pub height: Option<u32>,
    /// Marque de l'appareil.
    pub camera_make: Option<String>,
    /// Modèle de l'appareil.
    pub camera_model: Option<String>,
    /// Objectif.
    pub lens: Option<String>,
    /// Focale en millimètres.
    pub focal_length: Option<f64>,
    /// Ouverture (nombre f).
    pub aperture: Option<f64>,
    /// Temps de pose en secondes.
    pub exposure_time: Option<f64>,
    /// Sensibilité ISO.
    pub iso: Option<u32>,
    /// Vrai si le flash s'est déclenché.
    pub flash_fired: Option<bool>,
    /// Position GPS.
    pub gps: Option<GpsDto>,
    /// Durée d'une vidéo, en millisecondes.
    #[specta(type = Option<u32>)]
    pub duration_ms: Option<u64>,
}

/// Date de prise de vue des photos sélectionnées.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct CaptureTimeState {
    /// Date commune (`2024-07-14T18:30:05`), ou « valeurs mixtes ».
    pub value: FieldValueDto,
    /// Nombre de photos dont la date n'est plus celle d'origine.
    pub modified_count: u32,
}

/// Contenu du panneau Métadonnées.
#[derive(Debug, Clone, PartialEq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct MetadataPanel {
    /// Champs modifiables, dans l'ordre d'affichage.
    pub fields: Vec<FieldState>,
    /// Date de prise de vue, modifiable.
    pub capture_time: CaptureTimeState,
    /// Informations de la photo active ; `null` si elle n'est plus au
    /// catalogue.
    pub info: Option<MediaInfoDto>,
    /// La photo active est une version traitée : son original.
    pub edited_from: Option<EditedFromDto>,
}

/// Original d'une version traitée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct EditedFromDto {
    /// Original.
    pub id: u32,
    /// Son nom de fichier.
    pub file_name: String,
    /// Éditeur qui a produit la version ; `null` : inconnu.
    pub editor: Option<String>,
}

/// Contenu du panneau Métadonnées : valeurs des champs et date de prise de
/// vue pour les médias sélectionnés (« valeurs mixtes » si elles diffèrent)
/// et informations de la photo active.
///
/// # Errors
///
/// [`EditFailure::CatalogUnavailable`] si le catalogue est illisible.
// Tâche asynchrone : résumer 50 000 photos ne doit pas figer l'interface.
#[tauri::command]
#[specta::specta]
pub async fn get_metadata_panel(
    catalog: State<'_, Mutex<Catalog>>,
    active_id: u32,
    media_ids: Vec<u32>,
) -> Result<MetadataPanel, EditFailure> {
    let ids = media_ids_of(media_ids);
    let catalog = lock(&catalog);
    let connection = catalog.connection();
    let fields = metadata::summary(connection, &ids).map_err(|error| failure(&error))?;
    let (capture_time, modified_count) =
        capture_time::summary(connection, &ids).map_err(|error| failure(&error))?;
    let info = metadata::info(connection, MediaId::new(i64::from(active_id)))
        .map_err(|error| failure(&error))?;
    let edited_from = edited_from(connection, MediaId::new(i64::from(active_id)))
        .map_err(|error| failure(&error))?;
    Ok(MetadataPanel {
        fields: fields
            .into_iter()
            .map(|(field, summary)| FieldState {
                field: field.into(),
                value: summary.into(),
            })
            .collect(),
        capture_time: CaptureTimeState {
            value: capture_time.into(),
            modified_count: u32::try_from(modified_count).unwrap_or(u32::MAX),
        },
        info: info.map(MediaInfoDto::from),
        edited_from,
    })
}

/// Original de la version traitée `id`, s'il est toujours au catalogue.
fn edited_from(
    connection: &rusqlite::Connection,
    id: MediaId,
) -> Result<Option<EditedFromDto>, crate::catalog::CatalogError> {
    let Some(version) = edited_versions::get(connection, id)? else {
        return Ok(None);
    };
    let original =
        crate::catalog::repositories::media::read_with_paths(connection, &[version.original_id])?;
    Ok(original.into_iter().next().map(|(media, _)| EditedFromDto {
        id: u32::try_from(media.id.get()).unwrap_or(u32::MAX),
        file_name: media.data.file_name,
        editor: version.editor.as_deref().map(external_editing::editor_name),
    }))
}

/// Donne une valeur à un champ de métadonnées des médias indiqués, de façon
/// annulable. Un texte vide (ou fait d'espaces) vide le champ.
///
/// # Errors
///
/// Voir [`EditFailure`] ; rien n'est modifié en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn set_metadata(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    media_ids: Vec<u32>,
    field: MetadataFieldDto,
    value: String,
) -> Result<EditOutcome, EditFailure> {
    if value.chars().count() > MAX_METADATA_CHARS {
        return Err(EditFailure::InvalidValue);
    }
    let ids = media_ids_of(media_ids);
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    let summary = Edit::metadata(&catalog, field.into(), metadata_text(&value), &ids)
        .and_then(|edit| history.perform(&mut catalog, edit))
        .map_err(|error| failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

impl From<MetadataField> for MetadataFieldDto {
    fn from(field: MetadataField) -> Self {
        match field {
            MetadataField::Title => Self::Title,
            MetadataField::Caption => Self::Caption,
            MetadataField::Creator => Self::Creator,
            MetadataField::Copyright => Self::Copyright,
            MetadataField::Country => Self::Country,
            MetadataField::Region => Self::Region,
            MetadataField::City => Self::City,
            MetadataField::Location => Self::Location,
        }
    }
}

impl From<MetadataFieldDto> for MetadataField {
    fn from(field: MetadataFieldDto) -> Self {
        match field {
            MetadataFieldDto::Title => Self::Title,
            MetadataFieldDto::Caption => Self::Caption,
            MetadataFieldDto::Creator => Self::Creator,
            MetadataFieldDto::Copyright => Self::Copyright,
            MetadataFieldDto::Country => Self::Country,
            MetadataFieldDto::Region => Self::Region,
            MetadataFieldDto::City => Self::City,
            MetadataFieldDto::Location => Self::Location,
        }
    }
}

impl From<FieldSummary> for FieldValueDto {
    fn from(summary: FieldSummary) -> Self {
        match summary {
            FieldSummary::Same(value) => Self::Same { value },
            FieldSummary::Mixed => Self::Mixed,
        }
    }
}

impl From<MediaInfo> for MediaInfoDto {
    fn from(info: MediaInfo) -> Self {
        let details = info.details;
        Self {
            file_name: info
                .path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            folder: info
                .path
                .parent()
                .map(|folder| folder.to_string_lossy().into_owned())
                .unwrap_or_default(),
            // Exact jusqu'à 2⁵³ octets : bien au-delà de tout fichier réel.
            #[allow(clippy::cast_precision_loss)]
            file_size: info.file_size as f64,
            original_capture_time: (info.original_capture_time != info.capture_time)
                .then_some(info.original_capture_time),
            width: details.width,
            height: details.height,
            camera_make: details.camera_make,
            camera_model: details.camera_model,
            lens: details.lens,
            focal_length: details.focal_length_mm,
            aperture: details.aperture,
            exposure_time: details.exposure_time_s,
            iso: details.iso,
            flash_fired: details.flash_fired,
            gps: details.gps.map(|gps| GpsDto {
                latitude: gps.latitude,
                longitude: gps.longitude,
                altitude: gps.altitude,
            }),
            duration_ms: details.duration_ms,
        }
    }
}
