//! Changement d'emplacement de médias au catalogue (dossier, nom de
//! fichier) et de dossiers relocalisés, avec leur annulation : relocaliser
//! un fichier ou un dossier, et la part du catalogue d'un
//! déplacement ou d'un renommage.
//!
//! Une photo qui change de dossier quitte sa pile ; l'annulation
//! l'y remet. Un dossier vidé quitte le catalogue ; l'annulation le remet.

use std::path::PathBuf;

use rusqlite::{Connection, OptionalExtension, params};

use crate::catalog::CatalogError;
use crate::catalog::repositories::{folders, missing};
use crate::domain::{Folder, FolderId, MediaId, StackId};

// Chaque modification s'applique dans une transaction (voir `Edit::apply`) :
// un échec n'en laisse rien.

/// Emplacement d'un fichier : son dossier et son nom.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MediaPlace {
    /// Chemin complet du dossier.
    pub folder: PathBuf,
    /// Nom du fichier.
    pub file_name: String,
}

/// Un média qui change d'emplacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementChange {
    /// Média.
    pub media: MediaId,
    /// Emplacement avant.
    pub before: MediaPlace,
    /// Emplacement après.
    pub after: MediaPlace,
    /// Noté manquant avant.
    pub missing_before: bool,
    /// Noté manquant après.
    pub missing_after: bool,
    /// Place dans sa pile avant (retirée s'il change de dossier).
    pub stack: Option<(StackId, i64)>,
}

/// Médias qui changent d'emplacement.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlacementEdit {
    changes: Vec<PlacementChange>,
    /// Dossiers d'origine et leurs parents, à remettre à l'annulation.
    lineage: Vec<Folder>,
}

impl PlacementEdit {
    /// Prépare les changements `changes` : relève la place des médias dans
    /// leur pile et les dossiers d'origine.
    ///
    /// # Errors
    ///
    /// Échoue si la base est illisible.
    pub fn new(
        connection: &Connection,
        mut changes: Vec<PlacementChange>,
    ) -> Result<Self, CatalogError> {
        let mut origins = Vec::new();
        for change in &mut changes {
            change.stack = connection
                .prepare_cached("SELECT stack_id, position FROM stack_media WHERE media_id = ?1")?
                .query_row([change.media.get()], |row| {
                    Ok((StackId::new(row.get(0)?), row.get(1)?))
                })
                .optional()?;
            if let Some(folder) = folders::find_by_path(connection, &change.before.folder)? {
                origins.push(folder.id);
            }
        }
        Ok(Self {
            lineage: folders::lineage_of(connection, &origins)?,
            changes,
        })
    }

    /// Médias concernés.
    #[must_use]
    pub fn media_ids(&self) -> Vec<MediaId> {
        self.changes.iter().map(|change| change.media).collect()
    }

    /// Vrai si rien ne change.
    #[must_use]
    pub fn changes_nothing(&self) -> bool {
        self.changes.iter().all(|change| {
            change.before == change.after && change.missing_before == change.missing_after
        })
    }

    pub(crate) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        for change in &self.changes {
            place(
                connection,
                change.media,
                &change.after,
                change.missing_after,
            )?;
            if change.before.folder != change.after.folder && change.stack.is_some() {
                connection
                    .prepare_cached("DELETE FROM stack_media WHERE media_id = ?1")?
                    .execute([change.media.get()])?;
            }
        }
        folders::remove_without_media(connection)?;
        Ok(())
    }

    pub(crate) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        folders::restore_missing(connection, &self.lineage)?;
        for change in &self.changes {
            place(
                connection,
                change.media,
                &change.before,
                change.missing_before,
            )?;
            if let Some((stack, position)) = change.stack
                && change.before.folder != change.after.folder
            {
                connection
                    .prepare_cached(
                        "INSERT OR IGNORE INTO stack_media (media_id, stack_id, position)
                         SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM stacks WHERE id = ?2)",
                    )?
                    .execute(params![change.media.get(), stack.get(), position])?;
            }
        }
        folders::remove_without_media(connection)?;
        Ok(())
    }
}

/// Range le média dans `place` (le dossier est ajouté au catalogue au
/// besoin).
fn place(
    connection: &Connection,
    media: MediaId,
    place: &MediaPlace,
    is_missing: bool,
) -> Result<(), CatalogError> {
    let folder = folders::ensure_path(connection, &place.folder)?;
    connection
        .prepare_cached(
            "UPDATE media SET folder_id = ?2, file_name = ?3, file_missing = ?4 WHERE id = ?1",
        )?
        .execute(params![
            media.get(),
            folder.get(),
            place.file_name,
            is_missing
        ])?;
    Ok(())
}

/// Dossier relocalisé : son nouveau chemin (ses sous-dossiers suivent), et
/// l'état « manquant » de ses médias avant et après.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderRelocation {
    /// Dossier.
    pub folder: FolderId,
    /// Ancien chemin.
    pub from: PathBuf,
    /// Nouveau chemin.
    pub to: PathBuf,
    /// Ancien parent au catalogue.
    pub old_parent: Option<FolderId>,
    /// Nouveau parent au catalogue.
    pub new_parent: Option<FolderId>,
    /// Médias du dossier et de ses sous-dossiers.
    pub media: Vec<MediaId>,
    /// Ceux qui manquaient avant.
    pub missing_before: Vec<MediaId>,
    /// Ceux qui manquent après.
    pub missing_after: Vec<MediaId>,
}

impl FolderRelocation {
    pub(super) fn apply(&self, connection: &Connection) -> Result<(), CatalogError> {
        self.place(connection, &self.to, self.new_parent, &self.missing_after)
    }

    pub(super) fn revert(&self, connection: &Connection) -> Result<(), CatalogError> {
        self.place(
            connection,
            &self.from,
            self.old_parent,
            &self.missing_before,
        )
    }

    fn place(
        &self,
        connection: &Connection,
        path: &std::path::Path,
        parent: Option<FolderId>,
        missing_ids: &[MediaId],
    ) -> Result<(), CatalogError> {
        folders::relocate(connection, self.folder, path, parent)?;
        missing::mark(connection, &self.media, false)?;
        missing::mark(connection, missing_ids, true)?;
        Ok(())
    }
}
