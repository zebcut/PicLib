//! Import d'un dossier : par référence (les fichiers restent où ils sont) ou
//! par copie (les fichiers sont copiés, vérifiés et rangés par date).

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard, PoisonError};

use jiff::tz::TimeZone;

use super::copy::{self, CopyOptions, grouped_batches};
use super::folder_tree::FolderTree;
use super::outcome::{ImportError, ImportOptions, ImportReport};
use super::recovery::{self, FileRecovery};
use super::sidecars::Sidecars;
use crate::catalog::Catalog;
use crate::catalog::CatalogError;
use crate::catalog::repositories::file_operations::{
    self, OperationId, OperationKind, OperationStatus,
};
use crate::catalog::repositories::{folders, imports};
use crate::domain::{ImportCounts, ImportId, ImportMode, ImportStatus, Timestamp};
use crate::files::{ScannedFile, scan_source};
use crate::jobs::CancellationToken;
use crate::metadata::{ExtractedMetadata, MetadataError, read_metadata};
use crate::platform;
use backup::BackupState;

mod backup;
mod copying;
mod resume;
mod writing;

pub use resume::{RecoveredCopy, register_recovered_copies};

/// Importe par référence les photos et vidéos de `root` et de ses
/// sous-dossiers : les fichiers restent où ils sont et ne sont jamais
/// modifiés ; leurs métadonnées sont lues puis enregistrées au catalogue,
/// y compris celles posées par d'autres logiciels (fichiers annexes XMP, XMP intégré), puis le modèle de métadonnées choisi.
///
/// Les dates de fichier (faute de date EXIF) sont exprimées dans
/// `time_zone`. `on_progress` reçoit le nombre de fichiers traités et le
/// total. L'import est toujours enregistré au catalogue avec son état final
/// (terminé, annulé, en échec) et ses compteurs.
///
/// # Errors
///
/// Voir [`ImportError`].
pub fn import_by_reference(
    catalog: &Mutex<Catalog>,
    root: &Path,
    options: &ImportOptions,
    time_zone: &TimeZone,
    cancellation: &CancellationToken,
    on_progress: impl FnMut(u64, u64),
) -> Result<ImportReport, ImportError> {
    if platform::is_on_camera_card(root) {
        return Err(ImportError::CameraCard {
            path: root.to_path_buf(),
        });
    }
    run_import(
        catalog,
        root,
        None,
        options,
        time_zone,
        cancellation,
        on_progress,
    )
}

/// Importe par copie les photos et vidéos de `root` et de ses sous-dossiers
/// (carte mémoire comprise) : chaque fichier est copié et vérifié (taille
/// et empreinte SHA-256), avec ses fichiers annexes, dans
/// `destination/AAAA/MM/JJ` d'après sa date de prise de vue, sans jamais
/// écraser un fichier ; la source n'est jamais modifiée. Le catalogue
/// pointe vers les copies. Les doublons sont écartés avant d'être copiés.
///
/// Mêmes règles que [`import_by_reference`] pour les métadonnées, le
/// modèle, la progression et l'enregistrement de l'import ; chaque copie
/// est aussi notée au journal des opérations.
///
/// # Errors
///
/// Voir [`ImportError`].
pub fn import_by_copy(
    catalog: &Mutex<Catalog>,
    root: &Path,
    copy: &CopyOptions,
    options: &ImportOptions,
    time_zone: &TimeZone,
    cancellation: &CancellationToken,
    on_progress: impl FnMut(u64, u64),
) -> Result<ImportReport, ImportError> {
    copy::check_destination(root, &copy.destination)?;
    run_import(
        catalog,
        root,
        Some(copy),
        options,
        time_zone,
        cancellation,
        on_progress,
    )
}

fn run_import(
    catalog: &Mutex<Catalog>,
    root: &Path,
    copy: Option<&CopyOptions>,
    options: &ImportOptions,
    time_zone: &TimeZone,
    cancellation: &CancellationToken,
    mut on_progress: impl FnMut(u64, u64),
) -> Result<ImportReport, ImportError> {
    let scan = scan_source(root, &options.scan, cancellation, |_| {})?;
    let (import_id, operation, backup) = start_journal(catalog, root, copy)?;
    tracing::info!(
        import = import_id.get(),
        files = scan.files.len(),
        copy = copy.is_some(),
        "import started"
    );

    let mut session = Session {
        import_id,
        source_root: root,
        tree: FolderTree::new(copy.map_or(root, |copy| &copy.destination)),
        sidecars: Sidecars::new(&scan.files, &scan.sidecars),
        options,
        time_zone,
        counts: ImportCounts::default(),
        unreadable: scan.issues.into_iter().map(|issue| issue.path).collect(),
        copy: copy.zip(operation).map(|(options, operation)| CopyState {
            options,
            operation,
            digests: HashSet::new(),
            failed: Vec::new(),
            sidecars_not_copied: Vec::new(),
            sequence: 0,
            backup,
        }),
    };
    let files = selected_files(&scan.files, options);
    let outcome = session.run(catalog, &files, cancellation, &mut on_progress);

    let status = match &outcome {
        Ok(()) => ImportStatus::Completed,
        Err(ImportError::Cancelled) => ImportStatus::Cancelled,
        Err(_) => ImportStatus::Failed,
    };
    // L'état final est enregistré même après une annulation ou une erreur ;
    // les dossiers créés sans y ajouter de média (doublons, annulation)
    // quittent le catalogue.
    let backup_operation = session
        .copy
        .as_ref()
        .and_then(|state| state.backup.as_ref())
        .map(|backup| backup.operation);
    let operations: Vec<OperationId> = operation.into_iter().chain(backup_operation).collect();
    let recorded = record_end(
        catalog,
        import_id,
        &operations,
        status,
        &session.counts,
        options,
    );
    tracing::info!(import = import_id.get(), ?status, counts = ?session.counts, "import finished");

    outcome?;
    recorded?;
    let mut report = ImportReport {
        import_id,
        counts: session.counts,
        template_name: options
            .template
            .as_ref()
            .map(|template| template.name.clone()),
        unreadable: session.unreadable,
        destination: copy.map(|copy| copy.destination.clone()),
        copy_failed: Vec::new(),
        sidecars_not_copied: Vec::new(),
        backup_folder: None,
        backup_failed: Vec::new(),
    };
    if let Some(state) = session.copy {
        report.copy_failed = state.failed;
        report.sidecars_not_copied = state.sidecars_not_copied;
        if let Some(backup) = state.backup {
            report.backup_folder = Some(backup.folder);
            report.backup_failed = backup.failed;
        }
    }
    Ok(report)
}

/// Enregistre le début d'un import au catalogue : l'import, et pour une
/// copie, son opération au journal et celle de sa copie de sauvegarde.
fn start_journal(
    catalog: &Mutex<Catalog>,
    root: &Path,
    copy: Option<&CopyOptions>,
) -> Result<(ImportId, Option<OperationId>, Option<BackupState>), ImportError> {
    let catalog = lock(catalog);
    let connection = catalog.connection();
    let mode = if copy.is_some() {
        ImportMode::Copy
    } else {
        ImportMode::Reference
    };
    let import_id = imports::start(connection, mode, root, Timestamp::now())?;
    let Some(copy) = copy else {
        return Ok((import_id, None, None));
    };
    imports::set_destination(connection, import_id, &copy.destination)?;
    let operation = file_operations::start(
        connection,
        OperationKind::Copy,
        Some(import_id),
        Timestamp::now(),
    )?;
    let backup = match &copy.backup {
        Some(root) => Some(BackupState {
            folder: backup::backup_folder(root, &jiff::Zoned::now()),
            operation: file_operations::start(
                connection,
                OperationKind::Backup,
                Some(import_id),
                Timestamp::now(),
            )?,
            failed: Vec::new(),
            stopped: false,
        }),
        None => None,
    };
    Ok((import_id, Some(operation), backup))
}

/// Enregistre la fin d'un import (même annulé ou en échec) : son état et
/// ses compteurs, le journal de ses copies, les piles des fichiers importés,
/// et retire les dossiers créés sans média.
fn record_end(
    catalog: &Mutex<Catalog>,
    import_id: ImportId,
    operations: &[OperationId],
    status: ImportStatus,
    counts: &ImportCounts,
    options: &ImportOptions,
) -> Result<(), CatalogError> {
    let mut catalog = lock(catalog);
    let transaction = catalog.transaction()?;
    imports::finish(&transaction, import_id, status, counts, Timestamp::now())?;
    for &operation in operations {
        // Copies jamais vérifiées (annulation, erreur) : rien n'a été nommé
        // pour elles.
        file_operations::close_pending(&transaction, operation, "interrupted")?;
        file_operations::finish(
            &transaction,
            operation,
            operation_status(status),
            Timestamp::now(),
        )?;
    }
    // Même interrompu, ce qui a été importé est empilé.
    if options.stack_pairs {
        crate::stacks::stack_imported_pairs(&transaction, import_id)?;
    }
    // Les paires d'abord : chacune compte pour une photo de rafale.
    if let Some(gap) = options.burst_gap_seconds {
        crate::stacks::stack_imported_bursts(&transaction, import_id, gap)?;
    }
    folders::remove_without_media(&transaction)?;
    transaction.commit().map_err(CatalogError::from)
}

/// Fichiers à importer : ceux cochés dans l'écran d'import, sinon tous.
/// Les fichiers annexes sont reconnus parmi tous ceux de la source, même si
/// seuls certains fichiers sont importés.
fn selected_files(files: &[ScannedFile], options: &ImportOptions) -> Vec<ScannedFile> {
    match &options.only {
        Some(only) => files
            .iter()
            .filter(|file| only.contains(&file.path))
            .cloned()
            .collect(),
        None => files.to_vec(),
    }
}

fn operation_status(status: ImportStatus) -> OperationStatus {
    match status {
        ImportStatus::Completed | ImportStatus::Running => OperationStatus::Completed,
        ImportStatus::Cancelled => OperationStatus::Cancelled,
        ImportStatus::Failed => OperationStatus::Failed,
    }
}

/// État d'un import en cours.
struct Session<'a> {
    import_id: ImportId,
    /// Dossier importé (rangement « arborescence d'origine »).
    source_root: &'a Path,
    tree: FolderTree,
    sidecars: Sidecars,
    options: &'a ImportOptions,
    time_zone: &'a TimeZone,
    counts: ImportCounts,
    unreadable: Vec<PathBuf>,
    /// Import par copie : réglages et suivi des copies.
    copy: Option<CopyState<'a>>,
}

/// Suivi d'un import par copie.
struct CopyState<'a> {
    options: &'a CopyOptions,
    /// Opération du journal où chaque copie est notée.
    operation: OperationId,
    /// Empreintes des fichiers déjà copiés par cet import : deux fichiers
    /// identiques de la source ne sont copiés qu'une fois.
    digests: HashSet<String>,
    failed: Vec<PathBuf>,
    sidecars_not_copied: Vec<PathBuf>,
    /// Numéro du prochain groupe renommé.
    sequence: u32,
    /// Copie de sauvegarde des originaux.
    backup: Option<BackupState>,
}

/// Fichier prêt à être écrit : métadonnées lues, hors verrou du catalogue.
struct PreparedFile<'f> {
    file: &'f ScannedFile,
    file_name: String,
    metadata: ExtractedMetadata,
    metadata_unreadable: bool,
    recovery: FileRecovery,
}

/// Emplacement d'un fichier au catalogue : le sien (import par référence)
/// ou celui de sa copie.
struct Placed<'p, 'f> {
    file: &'p PreparedFile<'f>,
    /// Chemin enregistré au catalogue.
    path: PathBuf,
    /// Nom enregistré au catalogue (celui de la copie, suffixe compris).
    file_name: String,
    /// Empreinte du contenu, connue pour une copie.
    content_hash: Option<String>,
}

impl<'f> Session<'_> {
    fn run(
        &mut self,
        catalog: &Mutex<Catalog>,
        files: &'f [ScannedFile],
        cancellation: &CancellationToken,
        on_progress: &mut impl FnMut(u64, u64),
    ) -> Result<(), ImportError> {
        let total = files.len() as u64;
        let mut processed = 0_u64;
        on_progress(0, total);
        let batch_size = self.options.batch_size.max(1);
        let batches: Vec<Vec<&ScannedFile>> = if self.copy.is_some() {
            grouped_batches(files, batch_size)
        } else {
            files
                .chunks(batch_size)
                .map(|batch| batch.iter().collect())
                .collect()
        };
        for batch in &batches {
            // Les métadonnées sont lues sans bloquer le catalogue : c'est la
            // partie lente (lecture des fichiers).
            let prepared = if self.copy.is_some() {
                // A copy reports its progress as files are copied.
                self.prepare(batch, cancellation, |_| {})?
            } else {
                // By reference, reading is the whole work: it is reported
                // per file, or a batch could look stuck for minutes.
                self.prepare(batch, cancellation, |read| {
                    on_progress(processed + read, total);
                })?
            };
            if self.copy.is_some() {
                // Chaque groupe est inscrit au catalogue dès sa copie.
                self.copy_batch(catalog, &prepared, cancellation, |copied| {
                    processed += copied;
                    on_progress(processed, total);
                })?;
            } else {
                let placed: Vec<Placed<'_, 'f>> = prepared
                    .iter()
                    .map(|file| Placed {
                        file,
                        path: file.file.path.clone(),
                        file_name: file.file_name.clone(),
                        content_hash: None,
                    })
                    .collect();
                self.write(&mut lock(catalog), &placed, &[])?;
                processed += batch.len() as u64;
                on_progress(processed, total);
            }
        }
        Ok(())
    }

    /// Lit les métadonnées d'un lot (EXIF, XMP). Un fichier impossible à
    /// ouvrir est noté comme illisible et n'est pas importé ; un fichier aux
    /// métadonnées abîmées est importé avec ce qui a pu être lu.
    /// `on_read` receives the number of files of the batch read so far.
    fn prepare(
        &mut self,
        batch: &[&'f ScannedFile],
        cancellation: &CancellationToken,
        mut on_read: impl FnMut(u64),
    ) -> Result<Vec<PreparedFile<'f>>, ImportError> {
        let mut prepared = Vec::with_capacity(batch.len());
        for (read, &file) in batch.iter().enumerate() {
            if cancellation.is_cancelled() {
                return Err(ImportError::Cancelled);
            }
            on_read(read as u64);
            let Some(file_name) = file.path.file_name().and_then(|name| name.to_str()) else {
                tracing::warn!(path = %file.path.display(), "file name is not valid UTF-8, skipped");
                self.unreadable.push(file.path.clone());
                continue;
            };
            let (metadata, metadata_unreadable) = match read_metadata(&file.path, file.format) {
                Ok(metadata) => (metadata, false),
                Err(MetadataError::Unreadable(reason)) => {
                    tracing::warn!(path = %file.path.display(), %reason, "unreadable metadata");
                    (ExtractedMetadata::default(), true)
                }
                Err(MetadataError::Io(error)) => {
                    tracing::warn!(path = %file.path.display(), %error, "file cannot be read, skipped");
                    self.unreadable.push(file.path.clone());
                    continue;
                }
            };
            let recovery = recovery::recover(file, &metadata, &self.sidecars);
            prepared.push(PreparedFile {
                file,
                file_name: file_name.to_owned(),
                metadata_unreadable: metadata_unreadable || recovery.has_unreadable_metadata,
                metadata,
                recovery,
            });
        }
        Ok(prepared)
    }
}

/// Verrouille le catalogue. Un verrou « empoisonné » (panique d'un autre fil
/// pendant qu'il le tenait) protège quand même une connexion utilisable :
/// chaque écriture est dans une transaction, annulée en cas de panique.
fn lock(catalog: &Mutex<Catalog>) -> MutexGuard<'_, Catalog> {
    catalog.lock().unwrap_or_else(PoisonError::into_inner)
}
