//! Suivi des fichiers annexes XMP (table `xmp_sync`, migration 36) : ce qui reste à écrire, l'état du fichier que PicLib a
//! écrit ou lu en dernier, et ce qu'il faut savoir d'un média pour écrire
//! son XMP.

use std::collections::HashMap;
use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, Row, params, params_from_iter};

use crate::catalog::CatalogError;
use crate::catalog::sql::{
    color_label_from_text, flag_from_text, media_format_from_text, placeholders, rating_from_sql,
    rotation_from_sql, size_from_sql, size_to_sql, text_to_path,
};
use crate::domain::{
    ColorLabel, Flag, GpsPosition, MediaFormat, MediaId, MetadataField, Orientation, Rating,
    Rotation, Timestamp,
};
use crate::files::FileStamp;

/// Nombre maximal d'identifiants par requête.
const IDS_PER_QUERY: usize = 500;

/// Raison d'une écriture échouée, gardée jusqu'à la modification suivante.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum XmpError {
    /// La photo est introuvable.
    MissingPhoto,
    /// La photo est sur une carte mémoire : rien n'y est jamais écrit.
    CameraCard,
    /// Le XMP existant est illisible : il n'est pas remplacé.
    UnreadableSidecar,
    /// La fusion aurait changé autre chose que les propriétés de PicLib.
    NotPreserved,
    /// Le fichier n'a pas pu être écrit (droits, disque plein...).
    WriteFailed,
}

impl XmpError {
    fn text(self) -> &'static str {
        match self {
            Self::MissingPhoto => "missingPhoto",
            Self::CameraCard => "cameraCard",
            Self::UnreadableSidecar => "unreadableSidecar",
            Self::NotPreserved => "notPreserved",
            Self::WriteFailed => "writeFailed",
        }
    }
}

/// Fichier XMP que PicLib a écrit ou lu en dernier pour un média.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownSidecar {
    /// Nom du fichier, dans le dossier de la photo.
    pub name: String,
    /// Sa date et sa taille à ce moment-là.
    pub stamp: FileStamp,
    /// PicLib l'a écrit (sinon, il l'a seulement lu : il vient d'un autre
    /// logiciel).
    pub written: bool,
}

/// Mot-clé d'un média, avec ses parents : chaque niveau depuis la racine,
/// et s'il est inclus à l'export.
pub type KeywordChain = Vec<(String, bool)>;

/// Ce qu'il faut savoir d'un média pour écrire son XMP.
#[derive(Debug, Clone, PartialEq)]
pub struct XmpSource {
    /// Média.
    pub id: MediaId,
    /// Chemin de la photo.
    pub path: PathBuf,
    /// Format.
    pub format: MediaFormat,
    /// Fichier introuvable au dernier contrôle.
    pub missing: bool,
    /// Un autre RAW du même dossier porte le même nom sans l'extension : le
    /// nom `photo.xmp` ne peut pas être le sien seul.
    pub shares_stem_with_raw: bool,
    /// Note.
    pub rating: Rating,
    /// Marqueur.
    pub flag: Option<Flag>,
    /// Couleur.
    pub color_label: Option<ColorLabel>,
    /// Rotation PicLib.
    pub rotation: Rotation,
    /// Orientation EXIF du fichier.
    pub orientation: Option<Orientation>,
    /// Champs descriptifs non vides.
    pub fields: Vec<(MetadataField, String)>,
    /// Position GPS.
    pub gps: Option<GpsPosition>,
    /// Date de prise de vue (« 2024-07-14T10:00:00 ») et son décalage UTC.
    pub capture_time: (String, Option<i64>),
    /// Date lue à l'import, avant toute modification dans PicLib.
    pub original_capture_time: (String, Option<i64>),
    /// Mots-clés.
    pub keywords: Vec<KeywordChain>,
    /// Révision des métadonnées.
    pub revision: i64,
    /// Fichier XMP tel que PicLib l'a écrit ou lu en dernier.
    pub known: Option<KnownSidecar>,
    /// Le fichier a été modifié par un autre logiciel.
    pub changed_elsewhere: bool,
}

const DESCRIPTIVE_COLUMNS: [(MetadataField, &str); 8] = [
    (MetadataField::Title, "title"),
    (MetadataField::Caption, "caption"),
    (MetadataField::Creator, "creator"),
    (MetadataField::Copyright, "copyright"),
    (MetadataField::Country, "country"),
    (MetadataField::Region, "region"),
    (MetadataField::City, "city"),
    (MetadataField::Location, "location"),
];

/// Ce qu'il faut pour écrire le XMP des médias `ids` (les identifiants
/// inconnus sont ignorés).
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn sources(connection: &Connection, ids: &[MediaId]) -> Result<Vec<XmpSource>, CatalogError> {
    let mut sources: Vec<(XmpSource, i64)> = Vec::with_capacity(ids.len());
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "SELECT media.id, folders.path, media.file_name, media.format, media.file_missing,
                    media.rating, media.flag, media.color_label, media.rotation,
                    media.orientation, media.title, media.caption, media.creator,
                    media.copyright, media.country, media.region, media.city, media.location,
                    media.gps_latitude, media.gps_longitude, media.gps_altitude,
                    media.capture_time, media.capture_offset_seconds,
                    media.original_capture_time, media.original_capture_offset_seconds,
                    coalesce(xmp_sync.revision, 0), xmp_sync.file_modified_at,
                    xmp_sync.file_size, coalesce(xmp_sync.changed_elsewhere, 0),
                    media.folder_id, xmp_sync.sidecar_name,
                    coalesce(xmp_sync.file_written, 0)
             FROM media JOIN folders ON folders.id = media.folder_id
                LEFT JOIN xmp_sync ON xmp_sync.media_id = media.id
             WHERE media.id IN ({})",
            placeholders(1, chunk.len()),
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let rows = statement.query_map(params_from_iter(chunk.iter().map(|id| id.get())), read)?;
        for row in rows {
            let (source, folder) = row?;
            sources.push((source?, folder));
        }
    }
    let ids: Vec<MediaId> = sources.iter().map(|(source, _)| source.id).collect();
    let mut keywords = keyword_chains(connection, &ids)?;
    let mut complete = Vec::with_capacity(sources.len());
    for (mut source, folder) in sources {
        source.keywords = keywords.remove(&source.id).unwrap_or_default();
        if source.format.is_raw() {
            source.shares_stem_with_raw = shares_stem_with_raw(connection, &source, folder)?;
        }
        complete.push(source);
    }
    Ok(complete)
}

/// Vrai si un autre RAW du dossier `folder` a le même nom que `source`
/// sans l'extension (`DSC_1.NEF` et `DSC_1.DNG`), sans tenir compte des
/// majuscules.
fn shares_stem_with_raw(
    connection: &Connection,
    source: &XmpSource,
    folder: i64,
) -> Result<bool, CatalogError> {
    let stem_of = |name: &str| {
        std::path::Path::new(name)
            .file_stem()
            .map(|stem| stem.to_string_lossy().to_lowercase())
    };
    let Some(stem) = source
        .path
        .file_name()
        .and_then(|name| stem_of(&name.to_string_lossy()))
    else {
        return Ok(false);
    };
    // `LIKE` (sans majuscules) trouve les candidats ; le nom exact est
    // comparé ensuite (`DSC_1.part.NEF` n'a pas le nom `DSC_1`).
    let pattern = format!(
        "{}.%",
        stem.replace('\\', "\\\\")
            .replace('%', "\\%")
            .replace('_', "\\_")
    );
    let names = connection
        .prepare_cached(
            "SELECT file_name, format FROM media
             WHERE folder_id = ?1 AND id <> ?2 AND file_name LIKE ?3 ESCAPE '\\'",
        )?
        .query_map(params![folder, source.id.get(), pattern], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    for (name, format) in names {
        if stem_of(&name).as_deref() == Some(stem.as_str())
            && media_format_from_text(&format)?.is_raw()
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Une ligne de [`sources`] (sans les mots-clés), avec le dossier. L'erreur
/// extérieure vient de SQLite, l'intérieure d'une valeur inattendue.
type SourceRow = (Result<XmpSource, CatalogError>, i64);

fn read(row: &Row<'_>) -> rusqlite::Result<SourceRow> {
    let folder_path: String = row.get(1)?;
    let file_name: String = row.get(2)?;
    let format: String = row.get(3)?;
    let rating: i64 = row.get(5)?;
    let flag: Option<String> = row.get(6)?;
    let color: Option<String> = row.get(7)?;
    let rotation: i64 = row.get(8)?;
    let orientation: Option<u8> = row.get(9)?;
    let mut fields = Vec::new();
    for (index, (field, _)) in DESCRIPTIVE_COLUMNS.iter().enumerate() {
        let value: Option<String> = row.get(10 + index)?;
        if let Some(value) = value.filter(|value| !value.trim().is_empty()) {
            fields.push((*field, value));
        }
    }
    let latitude: Option<f64> = row.get(18)?;
    let longitude: Option<f64> = row.get(19)?;
    let altitude: Option<f64> = row.get(20)?;
    let modified_at: Option<i64> = row.get(26)?;
    let size: Option<i64> = row.get(27)?;
    let id = MediaId::new(row.get(0)?);
    let missing: bool = row.get(4)?;
    let capture_time = (row.get(21)?, row.get(22)?);
    let original_capture_time = (row.get(23)?, row.get(24)?);
    let revision = row.get(25)?;
    let changed_elsewhere: bool = row.get(28)?;
    let folder: i64 = row.get(29)?;
    let sidecar_name: Option<String> = row.get(30)?;
    let written: bool = row.get(31)?;
    let source = (|| {
        let known = known_sidecar(sidecar_name, modified_at, size, written)?;
        Ok(XmpSource {
            id,
            path: text_to_path(folder_path).join(file_name),
            format: media_format_from_text(&format)?,
            missing,
            shares_stem_with_raw: false,
            rating: rating_from_sql(rating)?,
            flag: flag.as_deref().map(flag_from_text).transpose()?,
            color_label: color.as_deref().map(color_label_from_text).transpose()?,
            rotation: rotation_from_sql(rotation)?,
            orientation: orientation.and_then(Orientation::new),
            fields,
            gps: latitude
                .zip(longitude)
                .map(|(latitude, longitude)| GpsPosition {
                    latitude,
                    longitude,
                    altitude,
                }),
            capture_time,
            original_capture_time,
            keywords: Vec::new(),
            revision,
            known,
            changed_elsewhere,
        })
    })();
    Ok((source, folder))
}

fn known_sidecar(
    name: Option<String>,
    modified_at: Option<i64>,
    size: Option<i64>,
    written: bool,
) -> Result<Option<KnownSidecar>, CatalogError> {
    let (Some(name), Some(modified_at), Some(size)) = (name, modified_at, size) else {
        return Ok(None);
    };
    Ok(Some(KnownSidecar {
        name,
        stamp: FileStamp {
            modified: Timestamp::from_unix_millis(modified_at),
            size: size_from_sql(size)?,
        },
        written,
    }))
}

/// Chaque mot-clé des médias `ids`, avec ses parents depuis la racine.
fn keyword_chains(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<HashMap<MediaId, Vec<KeywordChain>>, CatalogError> {
    let mut chains: HashMap<MediaId, Vec<KeywordChain>> = HashMap::new();
    for chunk in ids.chunks(IDS_PER_QUERY) {
        let sql = format!(
            "WITH RECURSIVE chain(media_id, leaf_id, keyword_id, depth) AS (
                 SELECT media_id, keyword_id, keyword_id, 0 FROM media_keywords
                 WHERE media_id IN ({})
                 UNION ALL
                 SELECT chain.media_id, chain.leaf_id, keywords.parent_id, chain.depth + 1
                 FROM chain JOIN keywords ON keywords.id = chain.keyword_id
                 WHERE keywords.parent_id IS NOT NULL
             )
             SELECT chain.media_id, chain.leaf_id, keywords.name, keywords.include_on_export
             FROM chain JOIN keywords ON keywords.id = chain.keyword_id
             ORDER BY chain.media_id, chain.leaf_id, chain.depth DESC",
            placeholders(1, chunk.len())
        );
        let mut statement = connection.prepare_cached(&sql)?;
        let mut rows = statement.query(params_from_iter(chunk.iter().map(|id| id.get())))?;
        let mut current: Option<(i64, i64)> = None;
        while let Some(row) = rows.next()? {
            let media: i64 = row.get(0)?;
            let leaf: i64 = row.get(1)?;
            let level = (row.get::<_, String>(2)?, row.get::<_, bool>(3)?);
            let list = chains.entry(MediaId::new(media)).or_default();
            if current == Some((media, leaf)) {
                if let Some(chain) = list.last_mut() {
                    chain.push(level);
                }
            } else {
                list.push(vec![level]);
                current = Some((media, leaf));
            }
        }
    }
    for list in chains.values_mut() {
        list.sort();
    }
    Ok(chains)
}

/// Note l'écriture réussie de la révision `revision` : le fichier `name`
/// est dans l'état `file`.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn record_written(
    connection: &Connection,
    id: MediaId,
    revision: i64,
    name: &str,
    file: FileStamp,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO xmp_sync (media_id, revision, written_revision, sidecar_name,
                 file_modified_at, file_size, file_written, changed_elsewhere, error)
             SELECT ?1, ?2, ?2, ?3, ?4, ?5, 1, 0, NULL
             WHERE EXISTS (SELECT 1 FROM media WHERE id = ?1)
             ON CONFLICT (media_id) DO UPDATE SET written_revision = ?2, sidecar_name = ?3,
                 file_modified_at = ?4, file_size = ?5, file_written = 1,
                 changed_elsewhere = 0, error = NULL",
        )?
        .execute(params![
            id.get(),
            revision,
            name,
            file.modified.unix_millis(),
            size_to_sql(file.size)?
        ])?;
    Ok(())
}

/// Note que le fichier `name` (écrit par un autre logiciel) vient d'être lu :
/// le catalogue est à jour avec lui, dans l'état `file`.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn record_read(
    connection: &Connection,
    id: MediaId,
    name: &str,
    file: FileStamp,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO xmp_sync (media_id, sidecar_name, file_modified_at, file_size,
                 file_written)
             SELECT ?1, ?2, ?3, ?4, 0 WHERE EXISTS (SELECT 1 FROM media WHERE id = ?1)
             ON CONFLICT (media_id) DO UPDATE SET written_revision = revision,
                 sidecar_name = ?2, file_modified_at = ?3, file_size = ?4, file_written = 0,
                 changed_elsewhere = 0, error = NULL",
        )?
        .execute(params![
            id.get(),
            name,
            file.modified.unix_millis(),
            size_to_sql(file.size)?
        ])?;
    Ok(())
}

/// Le fichier connu a disparu (supprimé par un autre logiciel) : il est
/// oublié, et le média redevient « à enregistrer ».
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn forget_file(connection: &Connection, id: MediaId) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "UPDATE xmp_sync SET sidecar_name = NULL, file_modified_at = NULL, file_size = NULL,
                 file_written = 0, changed_elsewhere = 0, revision = revision + 1
             WHERE media_id = ?1",
        )?
        .execute([id.get()])?;
    Ok(())
}

/// Fichiers XMP connus des médias du dossier `folder` et de ses
/// sous-dossiers : média, chemin de la photo, fichier connu.
///
/// # Errors
///
/// Échoue si la base est illisible ou contient une valeur inattendue.
pub fn known_in_folder(
    connection: &Connection,
    folder: i64,
) -> Result<Vec<(MediaId, PathBuf, KnownSidecar)>, CatalogError> {
    let rows = connection
        .prepare_cached(
            "WITH RECURSIVE branch(id) AS (
                 SELECT ?1
                 UNION
                 SELECT folders.id FROM folders JOIN branch ON folders.parent_id = branch.id
             )
             SELECT media.id, folders.path, media.file_name, xmp_sync.sidecar_name,
                    xmp_sync.file_modified_at, xmp_sync.file_size, xmp_sync.file_written
             FROM xmp_sync JOIN media ON media.id = xmp_sync.media_id
                JOIN folders ON folders.id = media.folder_id
             WHERE media.folder_id IN (SELECT id FROM branch)
                AND xmp_sync.sidecar_name IS NOT NULL",
        )?
        .query_map([folder], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, Option<String>>(3)?,
                row.get::<_, Option<i64>>(4)?,
                row.get::<_, Option<i64>>(5)?,
                row.get::<_, bool>(6)?,
            ))
        })?
        .collect::<Result<Vec<_>, _>>()?;
    let mut known = Vec::with_capacity(rows.len());
    for (id, folder_path, file_name, name, modified_at, size, written) in rows {
        if let Some(sidecar) = known_sidecar(name, modified_at, size, written)? {
            known.push((
                MediaId::new(id),
                text_to_path(folder_path).join(file_name),
                sidecar,
            ));
        }
    }
    Ok(known)
}

/// Note un échec d'écriture.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn record_error(
    connection: &Connection,
    id: MediaId,
    error: XmpError,
) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO xmp_sync (media_id, error)
             SELECT ?1, ?2 WHERE EXISTS (SELECT 1 FROM media WHERE id = ?1)
             ON CONFLICT (media_id) DO UPDATE SET error = ?2",
        )?
        .execute(params![id.get(), error.text()])?;
    Ok(())
}

/// Note que le fichier a été modifié par un autre logiciel.
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn record_changed_elsewhere(connection: &Connection, id: MediaId) -> Result<(), CatalogError> {
    connection
        .prepare_cached(
            "INSERT INTO xmp_sync (media_id, changed_elsewhere)
             SELECT ?1, 1 WHERE EXISTS (SELECT 1 FROM media WHERE id = ?1)
             ON CONFLICT (media_id) DO UPDATE SET changed_elsewhere = 1",
        )?
        .execute([id.get()])?;
    Ok(())
}

/// Rend « à enregistrer » les médias `ids` (une information écrite dans
/// leur XMP a changé sans passer par leurs colonnes : le chemin d'un
/// mot-clé renommé ou déplacé...).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn mark_changed(connection: &Connection, ids: &[MediaId]) -> Result<(), CatalogError> {
    let mut insert = connection.prepare_cached(
        "INSERT OR IGNORE INTO xmp_sync (media_id)
         SELECT ?1 WHERE EXISTS (SELECT 1 FROM media WHERE id = ?1)",
    )?;
    let mut bump = connection.prepare_cached(
        "UPDATE xmp_sync SET revision = revision + 1, error = NULL WHERE media_id = ?1",
    )?;
    for id in ids {
        insert.execute([id.get()])?;
        bump.execute([id.get()])?;
    }
    Ok(())
}

/// Médias qui portent le mot-clé `keyword` ou l'un de ses descendants.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn media_under_keyword(
    connection: &Connection,
    keyword: i64,
) -> Result<Vec<MediaId>, CatalogError> {
    let ids = connection
        .prepare_cached(
            "WITH RECURSIVE branch(id) AS (
                 SELECT ?1 UNION ALL
                 SELECT keywords.id FROM keywords JOIN branch ON keywords.parent_id = branch.id
             )
             SELECT DISTINCT media_id FROM media_keywords
             WHERE keyword_id IN (SELECT id FROM branch)",
        )?
        .query_map([keyword], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids.into_iter().map(MediaId::new).collect())
}

/// Les médias `ids` sont à jour avec leurs fichiers : ce que le catalogue
/// en sait vient d'être lu dans leurs fichiers (import).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn mark_in_sync(connection: &Connection, ids: &[MediaId]) -> Result<(), CatalogError> {
    let mut statement = connection
        .prepare_cached("UPDATE xmp_sync SET written_revision = revision WHERE media_id = ?1")?;
    for id in ids {
        statement.execute([id.get()])?;
    }
    Ok(())
}

/// Médias « à enregistrer » que l'écriture automatique peut écrire : ni
/// modifiés par un autre logiciel, ni en échec (une modification efface
/// l'échec), au plus `limit`.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn writable_pending(connection: &Connection, limit: u64) -> Result<Vec<MediaId>, CatalogError> {
    let limit = i64::try_from(limit).unwrap_or(i64::MAX);
    let ids = connection
        .prepare_cached(
            "SELECT media_id FROM xmp_sync
             WHERE revision <> written_revision AND changed_elsewhere = 0 AND error IS NULL
             ORDER BY media_id LIMIT ?1",
        )?
        .query_map([limit], |row| row.get::<_, i64>(0))?
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ids.into_iter().map(MediaId::new).collect())
}

/// Révision actuelle d'un média (0 s'il n'a jamais changé).
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn revision(connection: &Connection, id: MediaId) -> Result<i64, CatalogError> {
    Ok(connection
        .prepare_cached("SELECT revision FROM xmp_sync WHERE media_id = ?1")?
        .query_row([id.get()], |row| row.get(0))
        .optional()?
        .unwrap_or(0))
}
