//! Renommer des photos : un modèle fait d'éléments
//! (texte, date de prise de vue, nom d'origine, nom actuel, numéro,
//! appareil) donne le nouveau nom de base de chaque photo. L'autre fichier
//! d'une paire RAW + JPEG et les fichiers annexes gardent ce même nom de
//! base (`DSC_0002.NEF.dop` devient `Nouveau.NEF.dop`) ; un nom déjà pris
//! reçoit `-2`. L'exécution, le journal et Cmd+Z sont ceux du déplacement
//! (`moving`).

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use specta::Type;

use crate::catalog::CatalogError;
use crate::catalog::repositories::media;
use crate::domain::{CaptureTime, Media, MediaId};
use crate::moving::{
    MediaMoveKind, MovePlan, SourceGroup, free_stem, group_endings, planned_group, source_groups,
};

/// Format d'une date de prise de vue dans un nom.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub enum RenameDateFormat {
    /// `20240714`.
    Compact,
    /// `2024-07-14`.
    Dashed,
    /// `20240714-183005`.
    CompactWithTime,
    /// `2024-07-14_18-30-05`.
    DashedWithTime,
    /// `2024`.
    Year,
}

impl RenameDateFormat {
    fn pattern(self) -> &'static str {
        match self {
            Self::Compact => "%Y%m%d",
            Self::Dashed => "%Y-%m-%d",
            Self::CompactWithTime => "%Y%m%d-%H%M%S",
            Self::DashedWithTime => "%Y-%m-%d_%H-%M-%S",
            Self::Year => "%Y",
        }
    }
}

/// Élément d'un modèle de renommage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RenamePart {
    /// Texte libre.
    Text {
        /// Texte.
        text: String,
    },
    /// Date de prise de vue.
    CaptureDate {
        /// Format.
        format: RenameDateFormat,
    },
    /// Nom du fichier à l'import, sans extension.
    OriginalName,
    /// Nom actuel, sans extension.
    CurrentName,
    /// Numéro de séquence.
    Sequence,
    /// Modèle de l'appareil.
    Camera,
}

/// Modèle de renommage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct RenameTemplate {
    /// Éléments, dans l'ordre.
    pub parts: Vec<RenamePart>,
    /// Premier numéro de la séquence.
    pub sequence_start: u32,
    /// Nombre de chiffres du numéro (complété par des zéros).
    pub sequence_digits: u8,
}

/// Raison pour laquelle des photos ne peuvent pas être renommées.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RenameRefusal {
    /// Le modèle donne un nom vide.
    EmptyName,
    /// Les noms ne changent pas.
    NothingToRename,
    /// Le fichier d'une photo est introuvable.
    Missing,
    /// Une photo est sur une carte mémoire.
    CameraCard,
}

/// Nom de base sans extension.
fn stem(name: &str) -> String {
    Path::new(name).file_stem().map_or_else(
        || name.to_owned(),
        |stem| stem.to_string_lossy().into_owned(),
    )
}

/// Ce que le modèle peut reprendre d'une photo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RenameInput<'a> {
    /// Date de prise de vue.
    pub capture_time: &'a CaptureTime,
    /// Nom du fichier à l'import.
    pub original_name: &'a str,
    /// Nom actuel.
    pub current_name: &'a str,
    /// Modèle de l'appareil (à défaut, sa marque).
    pub camera: Option<&'a str>,
}

impl<'a> From<&'a Media> for RenameInput<'a> {
    fn from(media: &'a Media) -> Self {
        let data = &media.data;
        Self {
            capture_time: &data.capture_time,
            original_name: if data.original_file_name.is_empty() {
                &data.file_name
            } else {
                &data.original_file_name
            },
            current_name: &data.file_name,
            camera: data
                .details
                .camera_model
                .as_deref()
                .or(data.details.camera_make.as_deref()),
        }
    }
}

/// Nom de base que donne le modèle pour une photo, numéro `sequence` ; les
/// caractères interdits dans un nom de fichier sont remplacés par « - ».
#[must_use]
pub fn render(template: &RenameTemplate, input: RenameInput<'_>, sequence: u32) -> String {
    let text: String = template
        .parts
        .iter()
        .map(|part| match part {
            RenamePart::Text { text } => text.clone(),
            RenamePart::CaptureDate { format } => input
                .capture_time
                .local
                .strftime(format.pattern())
                .to_string(),
            RenamePart::OriginalName => stem(input.original_name),
            RenamePart::CurrentName => stem(input.current_name),
            RenamePart::Sequence => format!(
                "{:0width$}",
                template.sequence_start.saturating_add(sequence),
                width = usize::from(template.sequence_digits)
            ),
            RenamePart::Camera => input.camera.unwrap_or_default().to_owned(),
        })
        .collect();
    let cleaned: String = text
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' => '-',
            other if other.is_control() => '-',
            other => other,
        })
        .collect();
    cleaned.trim().trim_start_matches('.').trim().to_owned()
}

/// Prépare le renommage des photos `ids`, dans l'ordre de `ids` (celui de
/// la grille) pour la séquence ; une paire RAW + JPEG compte pour un.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn plan_rename(
    connection: &Connection,
    ids: &[MediaId],
    template: &RenameTemplate,
) -> Result<Result<MovePlan, RenameRefusal>, CatalogError> {
    let order: HashMap<MediaId, usize> = ids
        .iter()
        .enumerate()
        .map(|(index, id)| (*id, index))
        .collect();
    let mut groups = source_groups(connection, ids)?;
    let rank = |group: &SourceGroup| {
        group
            .media
            .iter()
            .filter_map(|(id, _)| order.get(id))
            .min()
            .copied()
            .unwrap_or(usize::MAX)
    };
    groups.sort_by_key(rank);
    let all: Vec<MediaId> = groups
        .iter()
        .flat_map(|group| group.media.iter().map(|(id, _)| *id))
        .collect();
    let by_id: HashMap<MediaId, Media> = media::read_with_paths(connection, &all)?
        .into_iter()
        .map(|(media, _)| (media.id, media))
        .collect();
    let mut taken = HashSet::new();
    let mut planned = Vec::new();
    for (sequence, group) in (0_u32..).zip(&groups) {
        if group
            .media
            .iter()
            .any(|(_, name)| !group.folder.join(name).is_file())
        {
            return Ok(Err(RenameRefusal::Missing));
        }
        if crate::platform::is_on_camera_card(&group.folder) {
            return Ok(Err(RenameRefusal::CameraCard));
        }
        // Le nom vient de la photo choisie la première dans le groupe.
        let Some(representative) = group
            .media
            .iter()
            .min_by_key(|(id, _)| order.get(id).copied().unwrap_or(usize::MAX))
            .and_then(|(id, _)| by_id.get(id))
        else {
            continue;
        };
        let base = render(template, representative.into(), sequence);
        if base.is_empty() {
            return Ok(Err(RenameRefusal::EmptyName));
        }
        if base == group.stem {
            continue;
        }
        let own: HashSet<PathBuf> = group.endings().into_iter().map(|(path, _)| path).collect();
        let new_stem = free_stem(
            connection,
            &group.folder,
            &base,
            &group_endings(group),
            &own,
            &mut taken,
        )?;
        if new_stem != group.stem {
            planned.push(planned_group(group, &group.folder, &new_stem));
        }
    }
    if planned.is_empty() {
        return Ok(Err(RenameRefusal::NothingToRename));
    }
    Ok(Ok(MovePlan {
        kind: MediaMoveKind::Renamed,
        groups: planned,
    }))
}
