//! « Lire depuis le fichier » : le XMP d'une photo,
//! modifié par un autre logiciel, remplace ce que le catalogue en sait, en
//! une seule modification annulable.
//!
//! Remplacés : la note, la couleur, les champs descriptifs et le lieu, les
//! mots-clés(le mot-clé de lieu suit les champs). Le marqueur et
//! la position GPS ne sont remplacés que si le fichier en donne : la
//! plupart des logiciels n'écrivent pas de marqueur, et la position peut
//! venir de l'EXIF de la photo.

use std::collections::{BTreeSet, HashMap};

use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::triage::{self, TriageValue};
use crate::catalog::repositories::{keywords, media_details, metadata, xmp_sync};
use crate::domain::{ColorLabel, Flag, GpsPosition, KeywordId, MediaId, MetadataField, Rating};
use crate::keywords::places::{self, PLACE_FIELDS, PlaceFields};
use crate::keywords::resolve_recovered;
use crate::metadata::recovered::RecoveredMetadata;

/// Champs descriptifs remplacés, dans l'ordre de [`MediaState::fields`].
const FIELDS: [MetadataField; 8] = [
    MetadataField::Title,
    MetadataField::Caption,
    MetadataField::Creator,
    MetadataField::Copyright,
    MetadataField::Country,
    MetadataField::Region,
    MetadataField::City,
    MetadataField::Location,
];

/// Ce qu'un fichier XMP dit d'un média.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarRead {
    /// Média.
    pub media_id: MediaId,
    /// Ce que le fichier contient.
    pub metadata: RecoveredMetadata,
}

/// Ce que la lecture remplace, pour un média.
#[derive(Debug, Clone, PartialEq)]
struct MediaState {
    rating: Rating,
    flag: Option<Flag>,
    color_label: Option<ColorLabel>,
    fields: [Option<String>; 8],
    gps: Option<GpsPosition>,
    keywords: BTreeSet<KeywordId>,
}

/// Médias dont le catalogue prend les valeurs de leur XMP.
#[derive(Debug, Clone, PartialEq)]
pub struct SidecarReadEdit {
    /// Média, état avant, état après.
    changes: Vec<(MediaId, MediaState, MediaState)>,
}

impl SidecarReadEdit {
    /// Prépare la lecture : lit l'état actuel des médias (les identifiants
    /// inconnus sont ignorés). Les mots-clés du fichier qui manquent sont
    /// créés (ils restent si la lecture est annulée, comme ceux d'une
    /// saisie).
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible ou ne peut pas être écrite.
    pub(super) fn new(
        connection: &Connection,
        reads: &[SidecarRead],
    ) -> Result<Self, CatalogError> {
        let ids: Vec<MediaId> = reads.iter().map(|read| read.media_id).collect();
        let current = current_states(connection, &ids)?;
        let before_places = places::read(connection, &ids)?;
        // Mot-clé de lieu posé aujourd'hui : il reste, sauf si le lieu change.
        let no_places: HashMap<MediaId, PlaceFields> =
            ids.iter().map(|id| (*id, PlaceFields::default())).collect();
        let current_place_keywords =
            places::changes(connection, &before_places, &no_places)?.unlink;
        let mut changes = Vec::with_capacity(reads.len());
        let mut after_places = HashMap::new();
        for read in reads {
            let Some(before) = current.get(&read.media_id) else {
                continue;
            };
            let after = after_state(connection, before, &read.metadata)?;
            let place = before_places
                .get(&read.media_id)
                .cloned()
                .unwrap_or_default();
            after_places.insert(
                read.media_id,
                PLACE_FIELDS.iter().fold(place, |place, field| {
                    let index = FIELDS.iter().position(|other| other == field);
                    let value = index.and_then(|index| after.fields[index].as_deref());
                    place.with(*field, value)
                }),
            );
            changes.push((read.media_id, before.clone(), after));
        }
        // Le mot-clé de lieu suit les nouveaux champs.
        let place_links = places::changes(connection, &before_places, &after_places)?;
        for (id, _, after) in &mut changes {
            for (media, keyword) in &current_place_keywords {
                if media == id {
                    after.keywords.insert(*keyword);
                }
            }
            for (media, keyword) in &place_links.unlink {
                if media == id {
                    after.keywords.remove(keyword);
                }
            }
            for (media, keyword) in &place_links.link {
                if media == id {
                    after.keywords.insert(*keyword);
                }
            }
        }
        Ok(Self { changes })
    }

    /// Médias concernés.
    pub(super) fn media_ids(&self) -> Vec<MediaId> {
        self.changes.iter().map(|(id, _, _)| *id).collect()
    }

    pub(super) fn changes_nothing(&self) -> bool {
        self.changes
            .iter()
            .all(|(_, before, after)| before == after)
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        for (id, before, after) in &self.changes {
            write_state(connection, *id, before, after)?;
        }
        Ok(())
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        for (id, before, after) in &self.changes {
            write_state(connection, *id, after, before)?;
        }
        Ok(())
    }
}

/// État des médias `ids` dans le catalogue.
fn current_states(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<HashMap<MediaId, MediaState>, CatalogError> {
    let mut links: HashMap<MediaId, BTreeSet<KeywordId>> = HashMap::new();
    for (media, keyword) in keywords::links(connection, ids)? {
        links.entry(media).or_default().insert(keyword);
    }
    Ok(xmp_sync::sources(connection, ids)?
        .into_iter()
        .map(|source| {
            let fields = FIELDS.map(|field| {
                source
                    .fields
                    .iter()
                    .find(|(other, _)| *other == field)
                    .map(|(_, value)| value.clone())
            });
            let state = MediaState {
                rating: source.rating,
                flag: source.flag,
                color_label: source.color_label,
                fields,
                gps: source.gps,
                keywords: links.remove(&source.id).unwrap_or_default(),
            };
            (source.id, state)
        })
        .collect())
}

/// État d'un média une fois son XMP lu.
fn after_state(
    connection: &Connection,
    before: &MediaState,
    read: &RecoveredMetadata,
) -> Result<MediaState, CatalogError> {
    Ok(MediaState {
        rating: read.rating.unwrap_or_default(),
        flag: read.flag.or(before.flag),
        color_label: read.color_label,
        fields: FIELDS.map(|field| read.field(field).map(str::to_owned)),
        gps: read.gps.or(before.gps),
        keywords: resolve_recovered(connection, &read.keywords)?
            .into_iter()
            .collect(),
    })
}

/// Fait passer un média de l'état `from` à l'état `to`.
fn write_state(
    connection: &Connection,
    id: MediaId,
    from: &MediaState,
    to: &MediaState,
) -> Result<(), CatalogError> {
    let ids = [id];
    triage::write(connection, TriageValue::Rating(to.rating), &ids)?;
    triage::write(connection, TriageValue::Flag(to.flag), &ids)?;
    triage::write(connection, TriageValue::ColorLabel(to.color_label), &ids)?;
    for (field, value) in FIELDS.iter().zip(&to.fields) {
        metadata::write(connection, *field, value.as_deref(), &ids)?;
    }
    media_details::set_gps(connection, id, to.gps)?;
    let removed: Vec<_> = from
        .keywords
        .difference(&to.keywords)
        .map(|keyword| (id, *keyword))
        .collect();
    let added: Vec<_> = to
        .keywords
        .difference(&from.keywords)
        .map(|keyword| (id, *keyword))
        .collect();
    keywords::unlink(connection, &removed)?;
    keywords::link(connection, &added)
}
