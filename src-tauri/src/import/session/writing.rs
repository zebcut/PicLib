//! Écriture d'un lot de fichiers importés au catalogue, avec leurs
//! métadonnées récupérées, le modèle choisi et, pour une copie, le chemin
//! d'origine dans la légende.

use crate::keywords::places;
use std::path::Path;

use rusqlite::Connection;

use super::super::recovery;
use super::copying::CopyDone;
use super::{ImportError, Placed, PreparedFile, Session};
use crate::catalog::repositories::file_operations::{self, ItemResult};
use crate::catalog::repositories::{media, metadata, xmp_sync};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{
    CaptureTime, CaptureTimeSource, FolderId, MediaDetails, MediaId, MediaKind, MetadataField,
    NewMedia,
};
use crate::metadata_templates;

impl Session<'_> {
    /// Écrit un lot dans une seule transaction, métadonnées récupérées et
    /// modèle de métadonnées compris : un fichier n'est jamais ajouté sans
    /// eux. Pour une copie, ses lignes du journal passent à « faites » dans
    /// la même transaction (`done`).
    pub(super) fn write(
        &mut self,
        catalog: &mut Catalog,
        placed: &[Placed<'_, '_>],
        done: &[CopyDone],
    ) -> Result<(), ImportError> {
        let transaction = catalog.transaction()?;
        // Entre deux lots, un dossier vide a pu quitter le catalogue (une
        // suppression faite pendant l'import) : les dossiers sont relus.
        self.tree.forget();
        let is_copy = self.copy.is_some();
        let mut added = Vec::with_capacity(placed.len());
        let mut origins: Vec<(MediaId, &Path)> = Vec::new();
        for entry in placed {
            let file = entry.file;
            let folder_path = entry.path.parent().unwrap_or(&entry.path);
            let folder_id = self.tree.folder_id(&transaction, folder_path)?;
            if media::exists_in_folder(&transaction, folder_id, &entry.file_name)? {
                self.counts.already_in_catalog += 1;
                continue;
            }

            let mut new_media = self.new_media(file, folder_id);
            new_media.file_name.clone_from(&entry.file_name);
            new_media.content_hash.clone_from(&entry.content_hash);
            // Import par référence : la transaction voit aussi les fichiers
            // déjà ajoutés par cet import, un doublon au sein du même dossier
            // est détecté. Import par copie : les doublons ont été écartés
            // avant d'être copiés.
            if !is_copy
                && self.options.skip_duplicates
                && media::find_duplicate(
                    &transaction,
                    &new_media.name_key(),
                    new_media.file_size,
                    &new_media.capture_time,
                )?
                .is_some()
            {
                self.counts.duplicates_skipped += 1;
                continue;
            }

            let id = media::insert(&transaction, &new_media)?;
            added.push(id);
            origins.push((id, &file.file.path));
            match new_media.kind() {
                MediaKind::Photo => self.counts.added_photos += 1,
                MediaKind::Video => self.counts.added_videos += 1,
            }
            if new_media.capture_time.source == CaptureTimeSource::FileModified {
                self.counts.without_exif_date += 1;
            }
            if file.metadata_unreadable {
                self.counts.unreadable_metadata += 1;
            }
            if recovery::write(&transaction, id, &file.recovery)? {
                self.counts.with_recovered_keywords += 1;
            }
            if file.recovery.has_conflict {
                self.counts.metadata_conflicts += 1;
            }
        }
        // Ce que le catalogue sait des médias ajoutés vient de leurs
        // fichiers : rien à écrire dans leurs XMP. Ce qui suit (modèle, mots-
        // clés de lieu, chemin d'origine) les rend « à enregistrer »
        //.
        xmp_sync::mark_in_sync(&transaction, &added)?;
        crate::xmp_files::remember_read_sidecars(&transaction, &added)?;
        // Après les métadonnées récupérées : le modèle l'emporte sur elles
        // (ses mots-clés s'ajoutent).
        if let Some(template) = &self.options.template {
            metadata_templates::apply_to_new_media(&transaction, template, &added)?;
        }
        // Les champs de lieu, récupérés ou posés par le modèle, deviennent
        // des mots-clés de lieu.
        places::link_current(&transaction, &added)?;
        // Enfin, le chemin d'origine à la suite de la légende.
        if self
            .copy
            .as_ref()
            .is_some_and(|state| state.options.original_path_in_caption)
        {
            append_original_paths(&transaction, &origins)?;
        }
        for copy in done {
            file_operations::complete(
                &transaction,
                copy.item,
                Some(&copy.path),
                ItemResult::Done,
                None,
            )?;
        }
        transaction.commit().map_err(CatalogError::from)?;
        Ok(())
    }

    /// Date de prise de vue d'un fichier : celle de l'EXIF, à défaut celle
    /// du XMP (la date d'origine des doublons reste celle de l'EXIF), à défaut la date du fichier.
    pub(super) fn capture_time(&self, file: &PreparedFile<'_>) -> CaptureTime {
        file.metadata
            .capture_time
            .or(file.recovery.metadata.capture_time)
            .unwrap_or_else(|| CaptureTime::from_file_modified(file.file.modified, self.time_zone))
    }

    fn new_media(&self, file: &PreparedFile<'_>, folder_id: FolderId) -> NewMedia {
        let details = MediaDetails {
            gps: file.recovery.metadata.gps,
            ..file.metadata.details.clone()
        };
        NewMedia {
            folder_id,
            file_name: file.file_name.clone(),
            original_file_name: file.file_name.clone(),
            format: file.file.format,
            file_size: file.file.size,
            file_modified_at: file.file.modified,
            capture_time: self.capture_time(file),
            details,
            import_id: self.import_id,
            content_hash: None,
        }
    }
}

/// Ajoute à la légende de chaque média le chemin d'origine de son fichier,
/// sur une nouvelle ligne si la légende n'est pas vide.
fn append_original_paths(
    connection: &Connection,
    origins: &[(MediaId, &Path)],
) -> Result<(), CatalogError> {
    for &(id, origin) in origins {
        let current = metadata::read(connection, MetadataField::Caption, &[id])?
            .into_iter()
            .next()
            .and_then(|(_, caption)| caption);
        let origin = origin.display().to_string();
        let caption = match current {
            Some(caption) => format!("{caption}\n{origin}"),
            None => origin,
        };
        metadata::write(connection, MetadataField::Caption, Some(&caption), &[id])?;
    }
    Ok(())
}
