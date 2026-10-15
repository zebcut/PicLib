//! Lecture du catalogue pour l'affichage : éléments d'une source (toutes les
//! photos, dernier import, photos d'un mot-clé ou d'un dossier), filtrés
//!(filtres Texte et Attribut) et triés, par
//! pages ;
//! fichier d'un média pour la Loupe.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, Row};

pub use super::library_query::{
    ColorChoice, FlagChoice, KindChoice, LibraryFilter, LibraryQuery, LibrarySort, LibrarySource,
    RatingComparison, RatingRule, SortKey,
};
pub use super::metadata_columns::{ColumnChoice, ColumnValue, MetadataColumn, column_values};
use super::quick_collection;
use super::stacks::{PLACE_COLUMNS, PLACE_JOINS};
pub use super::text_search::TextSearch;
use crate::catalog::CatalogError;
use crate::catalog::sql::{
    color_label_from_text, flag_from_text, media_format_from_text, media_kind_from_text,
    rating_from_sql, rotation_from_sql, size_from_sql, text_to_path,
};
use crate::domain::{
    ColorLabel, Flag, ImportId, MediaFormat, MediaId, MediaKind, Rating, Rotation, StackId,
    Timestamp,
};

/// État de la vignette d'un élément, tel que la grille en a besoin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThumbnailState {
    /// Pas encore fabriquée.
    Pending,
    /// Prête dans le cache.
    Ready,
    /// Impossible à fabriquer.
    Failed,
}

/// État du fichier XMP d'un média.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmpState {
    /// Des métadonnées ont changé depuis la dernière écriture.
    Pending,
    /// Le fichier a été modifié par un autre logiciel.
    ChangedElsewhere,
    /// La dernière écriture a échoué.
    Failed,
}

/// Élément de la grille.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryItem {
    /// Identifiant du média.
    pub id: MediaId,
    /// Photo ou vidéo.
    pub kind: MediaKind,
    /// Nom du fichier.
    pub file_name: String,
    /// Date de prise de vue locale (`2024-07-14T18:30:05`).
    pub capture_time: String,
    /// Durée d'une vidéo, en millisecondes.
    pub duration_ms: Option<u64>,
    /// État de la vignette.
    pub thumbnail: ThumbnailState,
    /// Marqueur.
    pub flag: Option<Flag>,
    /// Note.
    pub rating: Rating,
    /// Étiquette de couleur.
    pub color_label: Option<ColorLabel>,
    /// Rotation choisie par l'utilisateur.
    pub rotation: Rotation,
    /// Dans la collection cible (la collection rapide, par défaut).
    pub in_target: bool,
    /// Pile de la photo ; `None` hors d'une pile d'au moins deux photos.
    pub stack: Option<StackPlace>,
    /// Fichier introuvable à la dernière vérification.
    pub missing: bool,
    /// État de son fichier XMP ; `None` : à jour.
    pub xmp: Option<XmpState>,
    /// Date de modification du fichier, telle que le catalogue la connaît :
    /// elle change quand un éditeur remplace le fichier.
    pub file_modified_at: Timestamp,
}

/// Place d'une photo dans sa pile.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StackPlace {
    /// Pile.
    pub id: StackId,
    /// Repliée : la grille n'en montre qu'une photo.
    pub collapsed: bool,
    /// Nombre de photos de la pile.
    pub size: u32,
    /// Rang de la photo dans la pile, à partir de 1 (1 : la couverture).
    pub position: u32,
}

/// Fichier d'un média, pour l'afficher.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaFile {
    /// Chemin complet du fichier.
    pub path: PathBuf,
    /// Format du fichier.
    pub format: MediaFormat,
}

/// Nombre d'éléments affichés par la grille pour une source filtrée : une
/// pile repliée compte pour un.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn count(connection: &Connection, query: impl Into<LibraryQuery>) -> Result<u64, CatalogError> {
    count_sql(connection, &query.into().grid_sql(connection)?)
}

/// Nombre de photos et vidéos d'une source filtrée, piles comprises (les
/// panneaux comptent toutes les photos).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn count_media(
    connection: &Connection,
    query: impl Into<LibraryQuery>,
) -> Result<u64, CatalogError> {
    count_sql(connection, &query.into().sql(connection)?)
}

fn count_sql(
    connection: &Connection,
    sql: &super::library_query::QuerySql,
) -> Result<u64, CatalogError> {
    let count: i64 = connection
        .prepare_cached(&format!(
            "SELECT count(*) FROM media WHERE {}",
            sql.condition
        ))?
        .query_row(sql.params(&[]).as_slice(), |row| row.get(0))?;
    size_from_sql(count)
}

/// Une page d'éléments : au plus `limit` éléments à partir du rang `offset`.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn page(
    connection: &Connection,
    query: impl Into<LibraryQuery>,
    offset: u64,
    limit: u64,
) -> Result<Vec<LibraryItem>, CatalogError> {
    let sql = query.into().grid_sql(connection)?;
    let offset = i64::try_from(offset).unwrap_or(i64::MAX);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let mut statement = connection.prepare_cached(&format!(
        "SELECT media.id, media.kind, media.file_name, media.capture_time, media.duration_ms,
            thumbnails.status, media.flag, media.rating, media.color_label, media.rotation,
            {}, {PLACE_COLUMNS}, media.file_missing,
            CASE WHEN xmp_sync.changed_elsewhere = 1 THEN 'changedElsewhere'
                 WHEN xmp_sync.error IS NOT NULL THEN 'failed'
                 WHEN xmp_sync.revision <> xmp_sync.written_revision THEN 'pending'
            END,
            media.file_modified_at
         FROM media LEFT JOIN thumbnails ON thumbnails.media_id = media.id
            LEFT JOIN xmp_sync ON xmp_sync.media_id = media.id
         {PLACE_JOINS}
         WHERE {} {} LIMIT :limit OFFSET :offset",
        quick_collection::in_target_sql(connection)?,
        sql.condition,
        sql.order
    ))?;
    let rows = statement
        .query_map(
            sql.params(&[(":limit", &limit), (":offset", &offset)])
                .as_slice(),
            RawItem::read,
        )?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(LibraryItem::try_from).collect()
}

/// Identifiants des éléments `offset` à `offset + limit - 1` d'une source,
/// dans l'ordre de la grille : sert à sélectionner une plage ou tout, y
/// compris des éléments pas encore affichés.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn ids(
    connection: &Connection,
    query: impl Into<LibraryQuery>,
    offset: u64,
    limit: u64,
) -> Result<Vec<MediaId>, CatalogError> {
    let sql = query.into().grid_sql(connection)?;
    let offset = i64::try_from(offset).unwrap_or(i64::MAX);
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let ids = connection
        .prepare_cached(&format!(
            "SELECT media.id FROM media WHERE {} {} LIMIT :limit OFFSET :offset",
            sql.condition, sql.order
        ))?
        .query_map(
            sql.params(&[(":limit", &limit), (":offset", &offset)])
                .as_slice(),
            |row| row.get::<_, i64>(0),
        )?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids.into_iter().map(MediaId::new).collect())
}

/// Média rejeté d'une source, avec son fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RejectedMedia {
    /// Identifiant du média.
    pub id: MediaId,
    /// Nom du fichier.
    pub file_name: String,
    /// Chemin complet du fichier.
    pub path: PathBuf,
}

/// Médias marqués « rejetée » d'une source, filtrés, dans l'ordre de la
/// grille.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn rejected(
    connection: &Connection,
    query: impl Into<LibraryQuery>,
) -> Result<Vec<RejectedMedia>, CatalogError> {
    let sql = query.into().sql(connection)?;
    let rows = connection
        .prepare_cached(&format!(
            "SELECT media.id, media.file_name, folders.path
             FROM media JOIN folders ON folders.id = media.folder_id
             WHERE media.flag = 'rejected' AND {} {}",
            sql.condition, sql.order
        ))?
        .query_map(sql.params(&[]).as_slice(), |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .into_iter()
        .map(|(id, file_name, folder)| RejectedMedia {
            id: MediaId::new(id),
            path: text_to_path(folder).join(&file_name),
            file_name,
        })
        .collect())
}

/// Dernier import ayant ajouté au moins un élément au catalogue (« Import précédent »). Ceux qui n'ont ajouté que des versions traitées
/// par un éditeur ne comptent pas.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn latest_import(connection: &Connection) -> Result<Option<ImportId>, CatalogError> {
    let id: Option<i64> = connection
        .query_row(
            "SELECT max(id) FROM imports
             WHERE edited_versions = 0
               AND EXISTS (SELECT 1 FROM media WHERE media.import_id = imports.id)",
            [],
            |row| row.get(0),
        )
        .optional()?
        .flatten();
    Ok(id.map(ImportId::new))
}

/// Fichier d'un média ; `None` si le média n'est pas (ou plus) au catalogue.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn media_file(
    connection: &Connection,
    media_id: MediaId,
) -> Result<Option<MediaFile>, CatalogError> {
    let mut statement = connection.prepare_cached(
        "SELECT folders.path, media.file_name, media.format
         FROM media JOIN folders ON folders.id = media.folder_id
         WHERE media.id = ?1",
    )?;
    let row = statement
        .query_row([media_id.get()], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
            ))
        })
        .optional()?;
    row.map(|(folder, file_name, format)| {
        Ok(MediaFile {
            path: text_to_path(folder).join(file_name),
            format: media_format_from_text(&format)?,
        })
    })
    .transpose()
}

struct RawItem {
    id: i64,
    kind: String,
    file_name: String,
    capture_time: String,
    duration_ms: Option<i64>,
    thumbnail_status: Option<String>,
    flag: Option<String>,
    rating: i64,
    color_label: Option<String>,
    rotation: i64,
    in_target: bool,
    stack_id: Option<i64>,
    stack_collapsed: Option<bool>,
    stack_size: i64,
    stack_position: i64,
    missing: bool,
    xmp: Option<String>,
    file_modified_at: i64,
}

impl RawItem {
    fn read(row: &Row<'_>) -> rusqlite::Result<Self> {
        Ok(Self {
            id: row.get(0)?,
            kind: row.get(1)?,
            file_name: row.get(2)?,
            capture_time: row.get(3)?,
            duration_ms: row.get(4)?,
            thumbnail_status: row.get(5)?,
            flag: row.get(6)?,
            rating: row.get(7)?,
            color_label: row.get(8)?,
            rotation: row.get(9)?,
            in_target: row.get(10)?,
            stack_id: row.get(11)?,
            stack_collapsed: row.get(12)?,
            stack_size: row.get(13)?,
            stack_position: row.get(14)?,
            missing: row.get(15)?,
            xmp: row.get(16)?,
            file_modified_at: row.get(17)?,
        })
    }
}

impl TryFrom<RawItem> for LibraryItem {
    type Error = CatalogError;

    fn try_from(raw: RawItem) -> Result<Self, Self::Error> {
        Ok(Self {
            id: MediaId::new(raw.id),
            kind: media_kind_from_text(&raw.kind)?,
            file_name: raw.file_name,
            capture_time: raw.capture_time,
            duration_ms: raw.duration_ms.map(size_from_sql).transpose()?,
            thumbnail: match raw.thumbnail_status.as_deref() {
                None => ThumbnailState::Pending,
                Some("ready") => ThumbnailState::Ready,
                Some(_) => ThumbnailState::Failed,
            },
            flag: raw.flag.as_deref().map(flag_from_text).transpose()?,
            rating: rating_from_sql(raw.rating)?,
            color_label: raw
                .color_label
                .as_deref()
                .map(color_label_from_text)
                .transpose()?,
            rotation: rotation_from_sql(raw.rotation)?,
            in_target: raw.in_target,
            stack: match raw.stack_id {
                Some(id) if raw.stack_size >= 2 => Some(StackPlace {
                    id: StackId::new(id),
                    collapsed: raw.stack_collapsed.unwrap_or(false),
                    size: u32::try_from(raw.stack_size).unwrap_or(u32::MAX),
                    position: u32::try_from(raw.stack_position).unwrap_or(u32::MAX),
                }),
                _ => None,
            },
            missing: raw.missing,
            xmp: match raw.xmp.as_deref() {
                Some("pending") => Some(XmpState::Pending),
                Some("changedElsewhere") => Some(XmpState::ChangedElsewhere),
                Some(_) => Some(XmpState::Failed),
                None => None,
            },
            file_modified_at: Timestamp::from_unix_millis(raw.file_modified_at),
        })
    }
}
