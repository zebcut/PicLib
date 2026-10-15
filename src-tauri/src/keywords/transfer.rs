//! Import et export de la liste des mots-clés au format texte de Lightroom
//!.
//!
//! L'import se prépare d'abord (aperçu : ce qui sera créé, ce qui existe
//! déjà, les lignes ignorées), puis s'applique d'un bloc, annulable avec
//! Cmd+Z. L'export écrit tout l'arbre par le module `files`.

use std::path::Path;

use rusqlite::Connection;

use super::KeywordTree;
use super::text_format::{LineIssue, TextKeywordList, parse_keyword_list, write_keyword_list};
use crate::catalog::CatalogError;
use crate::catalog::repositories::keywords::{self, Keyword};
use crate::domain::KeywordId;
use crate::edits::{Edit, KeywordTreeChange, KeywordTreeEdit, KeywordTreeStep};
use crate::files::{TextFileError, Trash, read_text_file, write_text_file};

/// Taille maximale d'un fichier de mots-clés : bien au-delà d'une liste
/// réelle (quelques centaines de kilo-octets pour des dizaines de milliers
/// de mots-clés), mais évite de charger un fichier choisi par erreur.
pub const MAX_KEYWORD_FILE_BYTES: u64 = 10 * 1024 * 1024;

/// Import ou export impossible.
#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    /// Le fichier n'a pas pu être lu ou écrit.
    #[error(transparent)]
    File(#[from] TextFileError),
    /// Le catalogue n'a pas pu être lu.
    #[error(transparent)]
    Catalog(#[from] CatalogError),
}

/// Import préparé : ce qu'il créera, et ce qu'il laisse de côté.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    /// Mots-clés à créer, chaque parent avant ses enfants, avec leur futur
    /// identifiant.
    created: Vec<Keyword>,
    /// Mots-clés du fichier déjà présents au même endroit (réutilisés tels
    /// quels).
    pub existing: usize,
    /// Synonymes lus et ignorés (non gérés en version 1).
    pub synonyms: usize,
    /// Lignes ignorées.
    pub issues: Vec<LineIssue>,
}

impl ImportPlan {
    /// Nombre de mots-clés que l'import créera.
    #[must_use]
    pub fn created_count(&self) -> usize {
        self.created.len()
    }

    /// Modification annulable qui crée les mots-clés : l'annuler les
    /// supprime, les enfants avant leurs parents.
    #[must_use]
    pub fn into_edit(self) -> Edit {
        let count = self.created.len();
        let steps = self
            .created
            .into_iter()
            .map(KeywordTreeStep::Create)
            .collect();
        Edit::KeywordTree(KeywordTreeEdit::new(
            KeywordTreeChange::Imported(count),
            steps,
            Vec::new(),
        ))
    }
}

/// Lit un fichier de mots-clés.
///
/// # Errors
///
/// Voir [`TextFileError`] : fichier illisible, trop gros, ou pas en UTF-8.
pub fn read_keyword_file(path: &Path) -> Result<TextKeywordList, TextFileError> {
    Ok(parse_keyword_list(&read_text_file(
        path,
        MAX_KEYWORD_FILE_BYTES,
    )?))
}

/// Prépare l'import d'une liste lue : un mot-clé déjà présent au même
/// endroit (même parent, même nom sans tenir compte des majuscules) est
/// réutilisé et garde ses réglages ; les autres seront créés.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn plan_import(
    connection: &Connection,
    list: &TextKeywordList,
) -> Result<ImportPlan, CatalogError> {
    let tree = KeywordTree::new(keywords::all(connection)?);
    let mut next = keywords::next_id(connection)?.get();
    // Mot-clé du catalogue (existant ou à créer) pour chaque ligne lue.
    let mut resolved: Vec<KeywordId> = Vec::with_capacity(list.keywords.len());
    let mut created = Vec::new();
    let mut existing = 0;
    for keyword in &list.keywords {
        // Les parents sont lus avant leurs enfants : `resolved` les connaît.
        let parent = keyword
            .parent
            .and_then(|index| resolved.get(index).copied());
        let id = if let Some(id) = tree.child_named(parent, &keyword.name) {
            existing += 1;
            id
        } else {
            let id = KeywordId::new(next);
            next += 1;
            created.push(Keyword {
                id,
                parent_id: parent,
                name: keyword.name.clone(),
                include_on_export: keyword.include_on_export,
            });
            id
        };
        resolved.push(id);
    }
    Ok(ImportPlan {
        created,
        existing,
        synonyms: list.synonyms,
        issues: list.issues.clone(),
    })
}

/// Écrit toute la liste des mots-clés dans `path`. Un fichier déjà présent
/// part à la corbeille. Renvoie vrai si un fichier a été remplacé.
///
/// # Errors
///
/// Voir [`TransferError`] ; en cas d'échec, `path` n'est pas modifié.
pub fn export_keyword_list(
    connection: &Connection,
    trash: &dyn Trash,
    path: &Path,
) -> Result<bool, TransferError> {
    let tree = KeywordTree::new(keywords::all(connection)?);
    let line_ending = if cfg!(windows) { "\r\n" } else { "\n" };
    Ok(write_text_file(
        trash,
        path,
        &write_keyword_list(&tree, line_ending),
    )?)
}
