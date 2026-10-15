//! XMP : lecture des fichiers annexes (`photo.xmp`, `photo.NEF.xmp`) et des
//! paquets intégrés aux photos (repérés par `metadata::embedded`, en lecture seule), et préparation des fichiers annexes que PicLib
//! écrit (fusion qui garde tout ce que les autres logiciels y ont mis). L'écriture sur le disque passe par `files::write_sidecar`.

mod content;
mod fields;
mod merge;
mod namespaces;
mod rdf;

use std::path::Path;

pub use content::XmpContent;
pub use fields::XmpMetadata;

/// Taille maximale d'un fichier annexe : Lightroom y écrit aussi les
/// réglages de développement, bien plus volumineux que les métadonnées.
const MAX_SIDECAR_SIZE: u64 = 16 * 1024 * 1024;

/// XMP illisible.
#[derive(Debug, thiserror::Error)]
pub enum XmpError {
    /// Le fichier n'a pas pu être lu.
    #[error("cannot read the file: {0}")]
    Io(#[from] std::io::Error),
    /// Le fichier annexe est démesuré : il n'est pas lu.
    #[error("XMP too large")]
    TooLarge,
    /// Le XML est mal formé ou n'est pas en UTF-8.
    #[error("malformed XMP: {0}")]
    Malformed(String),
    /// La fusion aurait changé autre chose que les propriétés de PicLib, ou
    /// le paquet fusionné ne se relit pas comme prévu : rien n'est écrit.
    #[error("merged XMP does not check out: {0}")]
    NotPreserved(&'static str),
}

/// Lit un fichier annexe XMP.
///
/// # Errors
///
/// Voir [`XmpError`].
pub fn read_sidecar(path: &Path) -> Result<XmpMetadata, XmpError> {
    if std::fs::metadata(path)?.len() > MAX_SIDECAR_SIZE {
        return Err(XmpError::TooLarge);
    }
    parse_packet(&std::fs::read(path)?)
}

/// Taille maximale d'un fichier annexe lu avant d'y écrire.
pub const MAX_SIDECAR_BYTES: u64 = MAX_SIDECAR_SIZE;

/// Paquet XMP à écrire : `existing` (le fichier annexe actuel, s'il existe)
/// dont les propriétés de PicLib prennent les valeurs de `content`.
///
/// Deux vérifications avant de rendre le paquet : tout ce qui n'appartient
/// pas à PicLib est resté identique, et le paquet se relit avec les valeurs
/// de `content`.
///
/// # Errors
///
/// [`XmpError::Malformed`] si `existing` est illisible (il n'est alors pas
/// remplacé) ; [`XmpError::NotPreserved`] si une vérification échoue.
pub fn write_packet(existing: Option<&[u8]>, content: &XmpContent) -> Result<Vec<u8>, XmpError> {
    let content = &content.xml_safe();
    let source = match existing {
        Some(bytes) => std::str::from_utf8(bytes)
            .map_err(|_| XmpError::Malformed("not UTF-8".to_owned()))?
            .trim_start_matches('\u{feff}')
            .trim_end_matches('\0'),
        None => merge::EMPTY_PACKET,
    };
    let owned = content::owned(content);
    let merged = merge::merge(source, &owned, &content::properties(content))?;
    if merge::canonical(source, &owned)? != merge::canonical(&merged, &owned)? {
        return Err(XmpError::NotPreserved("foreign properties changed"));
    }
    if !reads_back(&parse_packet(merged.as_bytes())?, content) {
        return Err(XmpError::NotPreserved("written values do not read back"));
    }
    Ok(merged.into_bytes())
}

/// Vrai si le paquet relu donne les valeurs écrites (la note 0 ne se relit
/// pas : elle ne dit rien).
fn reads_back(read: &XmpMetadata, content: &XmpContent) -> bool {
    let metadata = &read.metadata;
    let gps_matches = match (metadata.gps, content.gps) {
        (None, None) => true,
        (Some(read), Some(written)) => {
            (read.latitude - written.latitude).abs() < 1e-6
                && (read.longitude - written.longitude).abs() < 1e-6
        }
        _ => false,
    };
    metadata.rating.map_or(0, crate::domain::Rating::stars) == content.rating
        && metadata.flag == content.flag
        && metadata.color_label == content.color_label
        && content
            .fields
            .iter()
            .all(|(field, value)| metadata.field(*field) == Some(value.as_str()))
        && gps_matches
        && metadata.orientation == content.orientation
        && read.sidecar_for_extension == content.sidecar_for_extension
}

/// Lit un paquet XMP (fichier annexe, ou bloc intégré à une photo).
///
/// # Errors
///
/// Échoue si le XML est mal formé ou n'est pas en UTF-8.
pub fn parse_packet(bytes: &[u8]) -> Result<XmpMetadata, XmpError> {
    let text =
        std::str::from_utf8(bytes).map_err(|_| XmpError::Malformed("not UTF-8".to_owned()))?;
    // Marque d'ordre des octets en tête, octets nuls de remplissage en fin
    // (balise TIFF).
    let text = text.trim_start_matches('\u{feff}').trim_end_matches('\0');
    Ok(fields::extract(&rdf::parse(text)?))
}
