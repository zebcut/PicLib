//! Import et export de la liste des mots-clés au format texte de Lightroom
//!. Les fenêtres de choix de fichier sont ouvertes par le cœur :
//! l'interface ne fournit que les textes traduits, jamais de chemin.
//!
//! L'import se fait en deux temps : un aperçu (fichier choisi, lu, comparé
//! à la liste), retenu ici, puis, si l'utilisateur confirme, l'import de ce
//! même contenu, annulable avec Cmd+Z.

use std::sync::Mutex;

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, State};

use super::dialogs;
use super::edits::{EditOutcome, failure, lock, outcome};
use crate::catalog::{Catalog, CatalogError};
use crate::edits::History;
use crate::files::{SystemTrash, TextFileError};
use crate::keywords::text_format::{LineIssueReason, TextKeywordList};
use crate::keywords::transfer::{
    MAX_KEYWORD_FILE_BYTES, TransferError, export_keyword_list, plan_import, read_keyword_file,
};

/// Lignes ignorées détaillées dans l'aperçu ; au-delà, seul leur nombre.
const MAX_LISTED_ISSUES: usize = 50;

/// Liste lue lors du dernier aperçu, en attente de confirmation.
#[derive(Debug, Default)]
pub struct PendingKeywordImport(Mutex<Option<TextKeywordList>>);

/// Aperçu d'un import, à confirmer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeywordImportPreviewDto {
    /// Nom du fichier choisi.
    pub file_name: String,
    /// Mots-clés que l'import créera.
    pub created: u32,
    /// Mots-clés du fichier déjà présents (réutilisés tels quels).
    pub existing: u32,
    /// Synonymes lus et ignorés.
    pub synonyms: u32,
    /// Nombre total de lignes ignorées.
    pub issue_count: u32,
    /// Les premières lignes ignorées, avec leur raison.
    pub issues: Vec<KeywordLineIssueDto>,
}

/// Ligne ignorée à l'import.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeywordLineIssueDto {
    /// Numéro de la ligne (à partir de 1).
    pub line: u32,
    /// Raison.
    pub reason: KeywordLineIssueReasonDto,
}

/// Raison pour laquelle une ligne est ignorée.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeywordLineIssueReasonDto {
    /// Décalée de plusieurs niveaux d'un coup.
    TooDeep,
    /// Son parent a été ignoré.
    ParentIgnored,
    /// Nom vide.
    EmptyName,
    /// Nom de plus de 200 caractères.
    TooLong,
    /// Nom contenant « , », « > » ou « | ».
    ForbiddenCharacter {
        /// Nom concerné.
        name: String,
    },
}

/// Export terminé.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(rename_all = "camelCase")]
pub struct KeywordExportDto {
    /// Nom du fichier écrit.
    pub file_name: String,
    /// Vrai si un fichier du même nom a été mis à la corbeille.
    pub replaced: bool,
}

/// Import ou export impossible.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum KeywordTransferFailure {
    /// Le catalogue n'est pas ouvert, ou n'a pas pu être lu ou écrit.
    CatalogUnavailable,
    /// L'emplacement choisi n'est pas un fichier du disque.
    UnsupportedLocation,
    /// Le fichier n'a pas pu être lu ou écrit.
    FileUnavailable,
    /// Le fichier dépasse la taille permise.
    TooLarge {
        /// Taille permise, en mégaoctets.
        megabytes: u32,
    },
    /// Le fichier n'est pas du texte UTF-8.
    NotUtf8,
    /// Le fichier existant n'a pas pu être mis à la corbeille ; rien n'a
    /// été écrit.
    CannotReplace,
    /// Aucun aperçu n'attend de confirmation.
    NothingToImport,
}

/// Fait choisir un fichier de mots-clés, le lit et le compare à la liste :
/// `null` si l'utilisateur ferme la fenêtre. Rien n'est encore importé.
///
/// # Errors
///
/// Voir [`KeywordTransferFailure`].
#[tauri::command]
#[specta::specta]
pub async fn preview_keyword_import(
    app: AppHandle,
    catalog: State<'_, Mutex<Catalog>>,
    pending: State<'_, PendingKeywordImport>,
    dialog_title: String,
    filter_name: String,
) -> Result<Option<KeywordImportPreviewDto>, KeywordTransferFailure> {
    let Some(path) = dialogs::pick_text_file(&app, dialog_title, &filter_name)
        .map_err(|_| KeywordTransferFailure::UnsupportedLocation)?
    else {
        return Ok(None);
    };
    let list = read_keyword_file(&path).map_err(|error| file_failure(&error))?;
    let plan =
        plan_import(lock(&catalog).connection(), &list).map_err(|error| catalog_failure(&error))?;
    *lock(&pending.0) = Some(list);
    Ok(Some(KeywordImportPreviewDto {
        file_name: path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        created: count(plan.created_count()),
        existing: count(plan.existing),
        synonyms: count(plan.synonyms),
        issue_count: count(plan.issues.len()),
        issues: plan
            .issues
            .into_iter()
            .take(MAX_LISTED_ISSUES)
            .map(|issue| KeywordLineIssueDto {
                line: count(issue.line),
                reason: issue.reason.into(),
            })
            .collect(),
    }))
}

/// Importe la liste du dernier aperçu, de façon annulable : les mots-clés
/// absents sont créés, ceux qui existent déjà restent tels quels.
///
/// # Errors
///
/// Voir [`KeywordTransferFailure`] ; rien n'est créé en cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn import_keywords(
    history: State<'_, Mutex<History>>,
    catalog: State<'_, Mutex<Catalog>>,
    pending: State<'_, PendingKeywordImport>,
) -> Result<EditOutcome, KeywordTransferFailure> {
    let list = lock(&pending.0)
        .take()
        .ok_or(KeywordTransferFailure::NothingToImport)?;
    // Toujours l'historique, puis le catalogue (voir `edits::set_triage`).
    let mut history = lock(&history);
    let mut catalog = lock(&catalog);
    // Préparé de nouveau : la liste a pu changer depuis l'aperçu.
    let plan = plan_import(catalog.connection(), &list).map_err(|error| catalog_failure(&error))?;
    let summary = history
        .perform(&mut catalog, plan.into_edit())
        .map_err(|error| catalog_failure(&error))?;
    Ok(outcome(Some(summary), &history))
}

/// Fait choisir où enregistrer la liste des mots-clés, puis l'écrit : `null`
/// si l'utilisateur ferme la fenêtre. Un fichier remplacé part à la
/// corbeille.
///
/// # Errors
///
/// Voir [`KeywordTransferFailure`] ; le fichier choisi n'est pas modifié en
/// cas d'échec.
#[tauri::command]
#[specta::specta]
pub async fn export_keywords(
    app: AppHandle,
    catalog: State<'_, Mutex<Catalog>>,
    dialog_title: String,
    filter_name: String,
    default_name: String,
) -> Result<Option<KeywordExportDto>, KeywordTransferFailure> {
    let Some(path) = dialogs::pick_text_save_path(&app, dialog_title, &filter_name, default_name)
        .map_err(|_| KeywordTransferFailure::UnsupportedLocation)?
    else {
        return Ok(None);
    };
    let replaced =
        export_keyword_list(lock(&catalog).connection(), &SystemTrash, &path).map_err(|error| {
            match error {
                TransferError::File(error) => file_failure(&error),
                TransferError::Catalog(error) => catalog_failure(&error),
            }
        })?;
    tracing::info!(replaced, "keyword list exported");
    Ok(Some(KeywordExportDto {
        file_name: path
            .file_name()
            .map_or_else(String::new, |name| name.to_string_lossy().into_owned()),
        replaced,
    }))
}

fn count(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}

fn catalog_failure(error: &CatalogError) -> KeywordTransferFailure {
    failure(error);
    KeywordTransferFailure::CatalogUnavailable
}

fn file_failure(error: &TextFileError) -> KeywordTransferFailure {
    match error {
        TextFileError::TooLarge { .. } => KeywordTransferFailure::TooLarge {
            megabytes: u32::try_from(MAX_KEYWORD_FILE_BYTES / (1024 * 1024)).unwrap_or(u32::MAX),
        },
        TextFileError::NotUtf8 => KeywordTransferFailure::NotUtf8,
        TextFileError::CannotReplace(reason) => {
            tracing::warn!(%reason, "cannot replace the keyword file");
            KeywordTransferFailure::CannotReplace
        }
        TextFileError::Mismatch | TextFileError::Io(_) => {
            tracing::error!(%error, "cannot read or write the keyword file");
            KeywordTransferFailure::FileUnavailable
        }
    }
}

impl From<LineIssueReason> for KeywordLineIssueReasonDto {
    fn from(reason: LineIssueReason) -> Self {
        match reason {
            LineIssueReason::TooDeep => Self::TooDeep,
            LineIssueReason::ParentIgnored => Self::ParentIgnored,
            LineIssueReason::EmptyName => Self::EmptyName,
            LineIssueReason::TooLong => Self::TooLong,
            LineIssueReason::ForbiddenCharacter(name) => Self::ForbiddenCharacter { name },
        }
    }
}
