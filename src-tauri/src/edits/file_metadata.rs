//! Métadonnées lues dans les fichiers de médias déjà au catalogue (« Lire les métadonnées des fichiers ») : seul ce qui manque est
//! complété, en une seule modification annulable. Ce que l'utilisateur a
//! saisi dans PicLib n'est jamais remplacé.

use crate::keywords::places::{self, PLACE_FIELDS, PlaceLinks};
use std::collections::{BTreeSet, HashMap, HashSet};

use jiff::tz::Offset;
use rusqlite::Connection;

use crate::catalog::CatalogError;
use crate::catalog::repositories::capture_time::{self, CaptureTimes, StoredCaptureTime};
use crate::catalog::repositories::triage::{self, TriageAttribute, TriageValue};
use crate::catalog::repositories::{keywords, media, media_details, metadata};
use crate::domain::{
    CaptureTimeSource, GpsPosition, KeywordId, Media, MediaDetails, MediaId, MetadataField, Rating,
    metadata_text,
};
use crate::keywords::resolve_recovered;
use crate::rereading::FoundMetadata;

/// Nouvelle date d'un média qui n'avait que la date de son fichier.
#[derive(Debug, Clone, PartialEq, Eq)]
struct DateFill {
    media_id: MediaId,
    before: CaptureTimes,
    after: CaptureTimes,
    /// Origine de la date trouvée.
    source: CaptureTimeSource,
}

/// Nombre de médias complétés, par sorte d'information.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FileMetadataCounts {
    /// Médias complétés, toutes informations confondues.
    pub media: usize,
    /// Note, marqueur ou couleur posés.
    pub triage: usize,
    /// Champs descriptifs remplis (titre, légende, auteur, lieu…).
    pub fields: usize,
    /// Mots-clés ajoutés.
    pub keywords: usize,
    /// Position GPS posée.
    pub gps: usize,
    /// Date de prise de vue trouvée à la place de celle du fichier.
    pub capture_time: usize,
    /// Informations techniques complétées (appareil, objectif, réglages…).
    pub details: usize,
}

/// Ce qui manquait aux médias et que leurs fichiers donnent.
#[derive(Debug, Clone, PartialEq)]
pub struct FileMetadataEdit {
    /// Attributs de tri posés (ils n'en avaient pas), par valeur.
    triage: Vec<(TriageValue, Vec<MediaId>)>,
    /// Champs remplis (ils étaient vides), par champ et valeur.
    fields: Vec<(MetadataField, String, Vec<MediaId>)>,
    /// Mots-clés ajoutés (ils n'étaient pas déjà posés).
    links: Vec<(MediaId, KeywordId)>,
    /// Positions posées sur des médias qui n'en avaient pas.
    gps: Vec<(MediaId, GpsPosition)>,
    dates: Vec<DateFill>,
    /// Informations techniques complétées : seules les valeurs écrites.
    details: Vec<(MediaId, MediaDetails)>,
    counts: FileMetadataCounts,
    /// Médias complétés, sans doublon.
    media: Vec<MediaId>,
    /// Mots-clés de lieu des champs de lieu remplis.
    places: PlaceLinks,
}

/// État actuel des médias concernés, lu en une fois.
struct Current {
    media: HashMap<MediaId, Media>,
    dates: HashMap<MediaId, CaptureTimes>,
    triage: HashMap<(MediaId, TriageAttribute), TriageValue>,
    fields: HashMap<(MediaId, MetadataField), String>,
    links: HashSet<(MediaId, KeywordId)>,
}

impl Current {
    fn read(connection: &Connection, ids: &[MediaId]) -> Result<Self, CatalogError> {
        let mut triage_values = HashMap::new();
        for attribute in [
            TriageAttribute::Rating,
            TriageAttribute::Flag,
            TriageAttribute::ColorLabel,
        ] {
            for (id, value) in triage::read(connection, attribute, ids)? {
                triage_values.insert((id, attribute), value);
            }
        }
        let mut fields = HashMap::new();
        for field in MetadataField::ALL {
            for (id, value) in metadata::read(connection, field, ids)? {
                if let Some(value) = value.as_deref().and_then(metadata_text) {
                    fields.insert((id, field), value);
                }
            }
        }
        Ok(Self {
            media: media::read_with_paths(connection, ids)?
                .into_iter()
                .map(|(media, _)| (media.id, media))
                .collect(),
            dates: capture_time::read(connection, ids)?.into_iter().collect(),
            triage: triage_values,
            fields,
            links: keywords::links(connection, ids)?.into_iter().collect(),
        })
    }

    /// Vrai si l'attribut n'est pas posé (sans note, sans marqueur, sans
    /// couleur).
    fn is_unset(&self, id: MediaId, attribute: TriageAttribute) -> bool {
        self.triage.get(&(id, attribute)) == Some(&unset(attribute))
    }
}

/// Valeur « non posé » d'un attribut de tri.
fn unset(attribute: TriageAttribute) -> TriageValue {
    match attribute {
        TriageAttribute::Rating => TriageValue::Rating(Rating::default()),
        TriageAttribute::Flag => TriageValue::Flag(None),
        TriageAttribute::ColorLabel => TriageValue::ColorLabel(None),
    }
}

impl FileMetadataEdit {
    /// Prépare ce qu'il faut compléter d'après `found` et l'état actuel du
    /// catalogue. Les mots-clés trouvés qui n'existent pas encore sont
    /// **créés** : à appeler dans une transaction (validée pour appliquer,
    /// abandonnée pour un simple aperçu). Les médias retirés entre-temps
    /// sont ignorés.
    ///
    /// # Errors
    ///
    /// Échoue si la base ne peut pas être lue, ou écrite (mots-clés créés).
    pub fn new(connection: &Connection, found: &[FoundMetadata]) -> Result<Self, CatalogError> {
        let ids: Vec<MediaId> = found.iter().map(|found| found.media_id).collect();
        let current = Current::read(connection, &ids)?;
        let mut builder = Builder::default();
        for found in found {
            if let Some(media) = current.media.get(&found.media_id) {
                builder.add(connection, &current, media, found)?;
            }
        }
        let mut edit = builder.finish();
        edit.places = edit.place_links(connection)?;
        Ok(edit)
    }

    /// Mots-clés de lieu à poser pour les champs de lieu remplis.
    fn place_links(&self, connection: &Connection) -> Result<PlaceLinks, CatalogError> {
        let filled: Vec<_> = self
            .fields
            .iter()
            .filter(|(field, _, _)| PLACE_FIELDS.contains(field))
            .collect();
        if filled.is_empty() {
            return Ok(PlaceLinks::default());
        }
        let ids: Vec<MediaId> = filled
            .iter()
            .flat_map(|(_, _, ids)| ids.iter().copied())
            .collect();
        let before = places::read(connection, &ids)?;
        let mut after = before.clone();
        for (field, value, ids) in filled {
            for id in ids {
                if let Some(fields) = after.get_mut(id) {
                    *fields = fields.with(*field, Some(value));
                }
            }
        }
        places::changes(connection, &before, &after)
    }

    /// Médias complétés.
    pub(super) fn media_ids(&self) -> Vec<MediaId> {
        self.media.clone()
    }

    /// Nombre de médias complétés, par sorte d'information.
    #[must_use]
    pub fn counts(&self) -> FileMetadataCounts {
        self.counts
    }

    /// Vrai si les fichiers n'apportent rien de nouveau.
    pub(super) fn changes_nothing(&self) -> bool {
        self.media.is_empty()
    }

    /// Vrai si des positions GPS sont posées (le lieu est ensuite cherché).
    #[must_use]
    pub fn sets_positions(&self) -> bool {
        !self.gps.is_empty()
    }

    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        for (value, ids) in &self.triage {
            triage::write(connection, *value, ids)?;
        }
        for (field, value, ids) in &self.fields {
            metadata::write(connection, *field, Some(value), ids)?;
        }
        // Pas de « dernière utilisation » : comme à l'import, ces mots-clés
        // n'ont pas été choisis dans PicLib(suggestions).
        keywords::link(connection, &self.links)?;
        for &(id, position) in &self.gps {
            media_details::set_gps(connection, id, Some(position))?;
        }
        for date in &self.dates {
            capture_time::write_with_original(connection, date.media_id, &date.after, date.source)?;
        }
        for (id, details) in &self.details {
            media_details::fill(connection, *id, details)?;
        }
        self.places.apply(connection)
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        self.places.revert(connection)?;
        for (id, details) in &self.details {
            media_details::clear(connection, *id, details)?;
        }
        for date in &self.dates {
            capture_time::write_with_original(
                connection,
                date.media_id,
                &date.before,
                CaptureTimeSource::FileModified,
            )?;
        }
        for &(id, _) in &self.gps {
            media_details::set_gps(connection, id, None)?;
        }
        keywords::unlink(connection, &self.links)?;
        for (field, _, ids) in &self.fields {
            metadata::write(connection, *field, None, ids)?;
        }
        for (value, ids) in &self.triage {
            triage::write(connection, unset(value.attribute()), ids)?;
        }
        Ok(())
    }
}

/// Construction de la modification, média par média.
#[derive(Default)]
struct Builder {
    triage: HashMap<TriageValue, Vec<MediaId>>,
    fields: HashMap<(MetadataField, String), Vec<MediaId>>,
    links: Vec<(MediaId, KeywordId)>,
    gps: Vec<(MediaId, GpsPosition)>,
    dates: Vec<DateFill>,
    details: Vec<(MediaId, MediaDetails)>,
    counts: FileMetadataCounts,
    media: BTreeSet<MediaId>,
}

impl Builder {
    fn add(
        &mut self,
        connection: &Connection,
        current: &Current,
        media: &Media,
        found: &FoundMetadata,
    ) -> Result<(), CatalogError> {
        let id = media.id;
        let recovered = &found.metadata;
        let triage_values = [
            (
                TriageAttribute::Rating,
                recovered.rating.map(TriageValue::Rating),
            ),
            (
                TriageAttribute::Flag,
                recovered.flag.map(|flag| TriageValue::Flag(Some(flag))),
            ),
            (
                TriageAttribute::ColorLabel,
                recovered
                    .color_label
                    .map(|color| TriageValue::ColorLabel(Some(color))),
            ),
        ];
        let mut completed = HashSet::new();
        for (attribute, value) in triage_values {
            if let Some(value) = value
                && value != unset(attribute)
                && current.is_unset(id, attribute)
            {
                self.triage.entry(value).or_default().push(id);
                completed.insert(Completed::Triage);
            }
        }
        for (field, value) in &recovered.fields {
            if !current.fields.contains_key(&(id, *field)) {
                self.fields
                    .entry((*field, value.clone()))
                    .or_default()
                    .push(id);
                completed.insert(Completed::Fields);
            }
        }
        for keyword in resolve_recovered(connection, &recovered.keywords)? {
            if !current.links.contains(&(id, keyword)) {
                self.links.push((id, keyword));
                completed.insert(Completed::Keywords);
            }
        }
        if media.data.details.gps.is_none()
            && let Some(position) = found.details.gps
        {
            self.gps.push((id, position));
            completed.insert(Completed::Gps);
        }
        if let Some(date) = date_fill(media, current.dates.get(&id), found) {
            self.dates.push(date);
            completed.insert(Completed::CaptureTime);
        }
        let details = missing_details(&media.data.details, &found.details);
        if details != MediaDetails::default() {
            self.details.push((id, details));
            completed.insert(Completed::Details);
        }
        if !completed.is_empty() {
            self.counts.media += 1;
            for kind in completed {
                self.counts.add(kind);
            }
            self.media.insert(id);
        }
        Ok(())
    }

    fn finish(self) -> FileMetadataEdit {
        FileMetadataEdit {
            triage: self.triage.into_iter().collect(),
            fields: self
                .fields
                .into_iter()
                .map(|((field, value), ids)| (field, value, ids))
                .collect(),
            links: self.links,
            gps: self.gps,
            dates: self.dates,
            details: self.details,
            counts: self.counts,
            media: self.media.into_iter().collect(),
            places: PlaceLinks::default(),
        }
    }
}

/// Sorte d'information complétée pour un média.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum Completed {
    Triage,
    Fields,
    Keywords,
    Gps,
    CaptureTime,
    Details,
}

impl FileMetadataCounts {
    fn add(&mut self, completed: Completed) {
        let count = match completed {
            Completed::Triage => &mut self.triage,
            Completed::Fields => &mut self.fields,
            Completed::Keywords => &mut self.keywords,
            Completed::Gps => &mut self.gps,
            Completed::CaptureTime => &mut self.capture_time,
            Completed::Details => &mut self.details,
        };
        *count += 1;
    }
}

/// Date trouvée dans les fichiers, pour un média qui n'a que la date de son
/// fichier et dont la date n'a pas été modifiée dans PicLib. Elle devient aussi sa date d'origine.
fn date_fill(
    media: &Media,
    times: Option<&CaptureTimes>,
    found: &FoundMetadata,
) -> Option<DateFill> {
    let times = times?;
    let date = found.capture_time?;
    if media.data.capture_time.source != CaptureTimeSource::FileModified
        || times.current != times.original
    {
        return None;
    }
    let stored = StoredCaptureTime {
        local: date.storage_text(),
        offset_seconds: date.offset.map(Offset::seconds),
    };
    (stored != times.current).then(|| DateFill {
        media_id: media.id,
        before: times.clone(),
        after: CaptureTimes {
            current: stored.clone(),
            original: stored,
        },
        source: date.source,
    })
}

/// Informations techniques que le catalogue n'a pas et que le fichier
/// donne (la position GPS a son propre traitement).
fn missing_details(known: &MediaDetails, found: &MediaDetails) -> MediaDetails {
    fn missing<T: Clone>(known: Option<&T>, found: Option<&T>) -> Option<T> {
        if known.is_some() {
            None
        } else {
            found.cloned()
        }
    }
    MediaDetails {
        width: missing(known.width.as_ref(), found.width.as_ref()),
        height: missing(known.height.as_ref(), found.height.as_ref()),
        orientation: missing(known.orientation.as_ref(), found.orientation.as_ref()),
        camera_make: missing(known.camera_make.as_ref(), found.camera_make.as_ref()),
        camera_model: missing(known.camera_model.as_ref(), found.camera_model.as_ref()),
        lens: missing(known.lens.as_ref(), found.lens.as_ref()),
        focal_length_mm: missing(
            known.focal_length_mm.as_ref(),
            found.focal_length_mm.as_ref(),
        ),
        aperture: missing(known.aperture.as_ref(), found.aperture.as_ref()),
        exposure_time_s: missing(
            known.exposure_time_s.as_ref(),
            found.exposure_time_s.as_ref(),
        ),
        iso: missing(known.iso.as_ref(), found.iso.as_ref()),
        flash_fired: missing(known.flash_fired.as_ref(), found.flash_fired.as_ref()),
        gps: None,
        duration_ms: missing(known.duration_ms.as_ref(), found.duration_ms.as_ref()),
    }
}
