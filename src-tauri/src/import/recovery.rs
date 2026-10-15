//! Métadonnées déjà posées sur les fichiers importés : lues dans
//! les fichiers annexes (XMP, `.dop` de DxO), le XMP et l'IPTC intégrés et
//! l'EXIF, fusionnées par ordre de priorité, puis écrites au catalogue avec
//! le fichier.

use rusqlite::Connection;

use super::sidecars::Sidecars;
use crate::catalog::CatalogError;
use crate::catalog::repositories::triage::{self, TriageValue};
use crate::catalog::repositories::{keywords, metadata, rotation};
use crate::domain::{MediaId, Rotation};
use crate::files::ScannedFile;
use crate::keywords::resolve_recovered;
use crate::metadata::embedded::{EmbeddedMetadata, read_embedded};
use crate::metadata::{
    ExtractedMetadata, RecoveredMetadata, Recovery, dop, merge, rotation_between, xmp,
};

/// Métadonnées récupérées pour un fichier, prêtes à être écrites.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FileRecovery {
    /// Informations retenues, source par source.
    pub metadata: RecoveredMetadata,
    /// Rotation faite dans un autre logiciel.
    pub rotation: Rotation,
    /// Vrai si deux sources se contredisaient.
    pub has_conflict: bool,
    /// Vrai si une source (XMP, `.dop`, IPTC) n'a pas pu être lue (les
    /// autres servent quand même).
    pub has_unreadable_metadata: bool,
}

/// Lit et fusionne les sources d'un fichier, de la plus prioritaire à la
/// moins prioritaire : fichier annexe XMP, fichier `.dop` de
/// DxO, XMP intégré, IPTC intégré (le XMP d'abord, comme le recommande la
/// norme MWG), EXIF (`extracted`, déjà lu). Lecture seule, sans le
/// catalogue.
#[must_use]
pub fn recover(
    file: &ScannedFile,
    extracted: &ExtractedMetadata,
    sidecars: &Sidecars,
) -> FileRecovery {
    let mut has_unreadable_metadata = false;
    let mut unreadable = |what: &str, error: &dyn std::fmt::Display| {
        tracing::warn!(path = %file.path.display(), %error, "unreadable {what}");
        has_unreadable_metadata = true;
    };

    let sidecar = sidecars
        .find_xmp(file)
        .and_then(|found| match xmp::read_sidecar(found.path) {
            Ok(xmp) if found.accepts(xmp.sidecar_for_extension.as_deref()) => Some(xmp),
            // Illisible, il ne compte que pour le média auquel il revient
            // sans doute (son indication d'extension n'a pas pu être lue).
            Err(error) if found.accepts(None) => {
                unreadable("XMP sidecar", &error);
                None
            }
            // Celui d'un autre média du même nom.
            Ok(_) | Err(_) => None,
        });
    let dop = sidecars.find_dop(file).and_then(|path| {
        let name = file.path.file_name()?.to_str()?;
        match dop::read_dop(path, name) {
            Ok(dop) => Some(dop),
            Err(error) => {
                unreadable("DxO sidecar", &error);
                None
            }
        }
    });
    let embedded = match read_embedded(&file.path, file.format) {
        Ok(embedded) => embedded,
        Err(error) => {
            unreadable("embedded metadata", &error);
            EmbeddedMetadata::default()
        }
    };
    for problem in &embedded.problems {
        unreadable("embedded metadata", problem);
    }

    let sources: Vec<RecoveredMetadata> = [
        sidecar.map(|xmp| xmp.metadata),
        dop,
        embedded.xmp,
        embedded.iptc,
        Some(extracted.recovered.clone()),
    ]
    .into_iter()
    .flatten()
    .collect();
    let Recovery {
        metadata,
        has_conflict,
    } = merge(&sources);
    FileRecovery {
        rotation: rotation_between(extracted.details.orientation, metadata.orientation),
        metadata,
        has_conflict,
        has_unreadable_metadata,
    }
}

/// Écrit ce qui a été récupéré pour un média qui vient d'être ajouté (sa
/// position GPS et sa date sont déjà enregistrées avec lui) ; renvoie vrai
/// si des mots-clés ont été posés. À appeler dans la transaction qui ajoute
/// le média, avant le modèle de métadonnées (qui l'emporte).
///
/// # Errors
///
/// Échoue si la base ne peut pas être écrite.
pub fn write(
    connection: &Connection,
    media: MediaId,
    recovery: &FileRecovery,
) -> Result<bool, CatalogError> {
    let ids = [media];
    let recovered = &recovery.metadata;
    let triage_values = [
        recovered.rating.map(TriageValue::Rating),
        recovered.flag.map(|flag| TriageValue::Flag(Some(flag))),
        recovered
            .color_label
            .map(|color| TriageValue::ColorLabel(Some(color))),
    ];
    for value in triage_values.into_iter().flatten() {
        triage::write(connection, value, &ids)?;
    }
    for (field, value) in &recovered.fields {
        metadata::write(connection, *field, Some(value), &ids)?;
    }
    if recovery.rotation != Rotation::default() {
        rotation::set(connection, &ids, recovery.rotation)?;
    }
    // Pas de « dernière utilisation » : ces mots-clés n'ont pas été choisis
    // dans PicLib, ils ne doivent pas remonter dans les suggestions.
    let found = resolve_recovered(connection, &recovered.keywords)?;
    let links: Vec<_> = found.iter().map(|&keyword| (media, keyword)).collect();
    keywords::link(connection, &links)?;
    Ok(!links.is_empty())
}
