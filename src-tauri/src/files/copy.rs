//! Copie vérifiée d'un fichier.
//!
//! La source est seulement lue. La copie est d'abord écrite sous un nom
//! provisoire caché dans le dossier de destination, puis relue et comparée
//! à la source (taille et empreinte SHA-256). Elle ne prend son vrai nom
//! qu'une fois vérifiée (relue depuis le disque, pas depuis la mémoire
//! cache du système), sans jamais écraser un fichier existant : un
//! fichier qui porte un nom définitif est donc toujours complet et vérifié.
//! Une copie inachevée (erreur, annulation, fichier écarté) ne laisse que
//! son fichier provisoire, effacé aussitôt.

use std::fmt;
use std::fs::{File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use sha2::{Digest, Sha256};

use crate::jobs::CancellationToken;
use crate::platform;

/// Taille des morceaux lus et écrits : assez grands pour les gros RAW et
/// les vidéos, assez petits pour réagir vite à une annulation.
const CHUNK_SIZE: usize = 1024 * 1024;

/// Préfixe des fichiers provisoires : caché (point initial), donc ignoré
/// par le parcours des dossiers.
const PARTIAL_PREFIX: &str = ".piclib-copie-";

/// Numéro des fichiers provisoires de ce processus.
static PARTIAL_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Empreinte SHA-256 du contenu d'un fichier.
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ContentDigest([u8; 32]);

impl ContentDigest {
    /// Écriture hexadécimale en minuscules (64 caractères), telle
    /// qu'enregistrée au catalogue.
    #[must_use]
    pub fn to_hex(&self) -> String {
        use fmt::Write as _;
        self.0
            .iter()
            .fold(String::with_capacity(64), |mut text, byte| {
                let _ = write!(text, "{byte:02x}");
                text
            })
    }
}

impl fmt::Debug for ContentDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "ContentDigest({})", self.to_hex())
    }
}

/// Raison de l'échec d'une copie. La source n'est jamais touchée.
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
    /// La source n'a pas pu être lue (fichier disparu, carte retirée...).
    #[error("cannot read {}: {source}", path.display())]
    Source {
        /// Fichier source.
        path: PathBuf,
        /// Erreur du système.
        source: io::Error,
    },
    /// La destination n'a pas pu être écrite (disque plein, droits...).
    #[error("cannot write in {}: {source}", path.display())]
    Destination {
        /// Dossier ou fichier de destination.
        path: PathBuf,
        /// Erreur du système.
        source: io::Error,
    },
    /// La copie relue ne correspond pas à la source.
    #[error("the copy of {} does not match its source", path.display())]
    Mismatch {
        /// Fichier source.
        path: PathBuf,
    },
    /// Copie annulée par l'utilisateur.
    #[error("copy cancelled")]
    Cancelled,
}

impl CopyError {
    /// Vrai si la destination est pleine : inutile alors de poursuivre.
    #[must_use]
    pub fn is_storage_full(&self) -> bool {
        matches!(self, Self::Destination { source, .. }
            if source.kind() == io::ErrorKind::StorageFull)
    }
}

/// Copie vérifiée, encore sous son nom provisoire : à nommer avec
/// [`PendingCopy::place`], sinon effacée dès qu'elle est abandonnée.
#[derive(Debug)]
pub struct PendingCopy {
    partial: PathBuf,
    size: u64,
    digest: ContentDigest,
    /// Faux une fois nommée : il n'y a plus rien à effacer.
    pending: bool,
}

impl PendingCopy {
    /// Taille du fichier copié, en octets.
    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    /// Empreinte du contenu (identique pour la source et la copie).
    #[must_use]
    pub fn digest(&self) -> ContentDigest {
        self.digest
    }

    /// Donne à la copie son nom définitif `target`, dans son dossier de
    /// destination, sans jamais écraser un fichier existant.
    ///
    /// # Errors
    ///
    /// Si `target` existe déjà, la copie est rendue intacte
    /// ([`PlaceError::Taken`]) pour être nommée autrement ; toute autre
    /// erreur l'abandonne (fichier provisoire effacé).
    pub fn place(mut self, target: &Path) -> Result<PathBuf, PlaceError> {
        match link_without_overwrite(&self.partial, target) {
            Ok(()) => {
                self.remove_partial();
                self.pending = false;
                Ok(target.to_path_buf())
            }
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                Err(PlaceError::Taken(self))
            }
            Err(source) => Err(PlaceError::Failed(CopyError::Destination {
                path: target.to_path_buf(),
                source,
            })),
        }
    }

    fn remove_partial(&self) {
        // Fichier provisoire créé par cette copie : ce n'est pas un fichier
        // de l'utilisateur (« supprimer le fichier partiellement copié »).
        if let Err(error) = std::fs::remove_file(&self.partial)
            && error.kind() != io::ErrorKind::NotFound
        {
            tracing::warn!(path = %self.partial.display(), %error, "cannot remove a partial copy");
        }
    }
}

impl Drop for PendingCopy {
    fn drop(&mut self) {
        if self.pending {
            self.remove_partial();
        }
    }
}

/// Échec du nommage d'une copie.
#[derive(Debug)]
pub enum PlaceError {
    /// Le nom est déjà pris : la copie est rendue, à nommer autrement.
    Taken(PendingCopy),
    /// Échec d'écriture : la copie est abandonnée.
    Failed(CopyError),
}

/// Copie `source` dans `destination_dir` (créé au besoin) sous un nom
/// provisoire, puis relit la copie pour la comparer à la source. La date de
/// modification de la source est reportée sur la copie.
///
/// # Errors
///
/// Voir [`CopyError`] ; le fichier provisoire est alors déjà effacé.
pub fn copy_verified(
    source: &Path,
    destination_dir: &Path,
    cancellation: &CancellationToken,
) -> Result<PendingCopy, CopyError> {
    let destination_error = |source: io::Error| CopyError::Destination {
        path: destination_dir.to_path_buf(),
        source,
    };
    std::fs::create_dir_all(destination_dir).map_err(destination_error)?;

    let source_error = |error: io::Error| CopyError::Source {
        path: source.to_path_buf(),
        source: error,
    };
    let mut input = File::open(source).map_err(source_error)?;
    let modified = input
        .metadata()
        .and_then(|metadata| metadata.modified())
        .map_err(source_error)?;

    let (partial, mut output) = create_partial(destination_dir).map_err(destination_error)?;
    // À partir d'ici, toute sortie anticipée efface le fichier provisoire.
    let mut pending = PendingCopy {
        partial,
        size: 0,
        digest: ContentDigest([0; 32]),
        pending: true,
    };

    // Ce qui est écrit ne reste pas en mémoire : la relecture qui suit lira
    // le disque, et une grosse copie ne chasse pas le reste du cache.
    platform::disable_cache(&output).map_err(destination_error)?;

    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK_SIZE];
    loop {
        if cancellation.is_cancelled() {
            return Err(CopyError::Cancelled);
        }
        let read = match input.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(source_error(error)),
        };
        hasher.update(&buffer[..read]);
        output
            .write_all(&buffer[..read])
            .map_err(destination_error)?;
        pending.size += read as u64;
    }
    output.set_modified(modified).map_err(destination_error)?;
    output.sync_all().map_err(destination_error)?;
    drop(output);

    pending.digest = ContentDigest(hasher.finalize().into());
    let (copied_size, copied_digest) =
        digest_of(&pending.partial, cancellation).map_err(|error| match error {
            DigestError::Cancelled => CopyError::Cancelled,
            DigestError::Io(source) => CopyError::Destination {
                path: pending.partial.clone(),
                source,
            },
        })?;
    if copied_size != pending.size || copied_digest != pending.digest {
        return Err(CopyError::Mismatch {
            path: source.to_path_buf(),
        });
    }
    Ok(pending)
}

/// Crée un fichier provisoire vide, au nom inédit, dans `dir`.
fn create_partial(dir: &Path) -> io::Result<(PathBuf, File)> {
    loop {
        let number = PARTIAL_COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = dir.join(format!(
            "{PARTIAL_PREFIX}{}-{number}.part",
            std::process::id()
        ));
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {}
            Err(error) => return Err(error),
        }
    }
}

/// Donne au fichier `from` le nom `to` sans jamais écraser : un lien dur
/// échoue si `to` existe (création exclusive). Sur un disque sans liens
/// durs (`ExFAT`, partage réseau...), renommage exclusif ; si le disque ne
/// le permet pas non plus (`ExFAT` sous macOS), le nom est vérifié juste
/// avant de renommer : seul un fichier créé dans cet instant, par un autre
/// programme, pourrait alors être remplacé. `from` doit être un fichier
/// provisoire de PicLib.
///
/// # Errors
///
/// `ErrorKind::AlreadyExists` si `to` existe ; sinon l'erreur du système.
pub(super) fn link_without_overwrite(from: &Path, to: &Path) -> io::Result<()> {
    match std::fs::hard_link(from, to) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::AlreadyExists => Err(error),
        Err(_) => match platform::rename_exclusive(from, to) {
            Err(error) if error.kind() == io::ErrorKind::Unsupported => {
                if to.symlink_metadata().is_ok() {
                    return Err(io::Error::from(io::ErrorKind::AlreadyExists));
                }
                std::fs::rename(from, to)
            }
            other => other,
        },
    }
}

enum DigestError {
    Io(io::Error),
    Cancelled,
}

/// Taille et empreinte d'un fichier existant, relu depuis le disque : pour
/// reconnaître, après un arrêt brutal, une copie vérifiée puis nommée.
///
/// # Errors
///
/// Échoue si le fichier est introuvable ou illisible.
pub fn file_digest(path: &Path) -> io::Result<(u64, ContentDigest)> {
    match digest_of(path, &CancellationToken::default()) {
        Ok(found) => Ok(found),
        Err(DigestError::Io(error)) => Err(error),
        Err(DigestError::Cancelled) => Err(io::Error::from(io::ErrorKind::Interrupted)),
    }
}

/// Efface les fichiers provisoires de copie (`.piclib-copie-…part`) laissés
/// dans `folder` par un arrêt brutal ; renvoie leur nombre. Ce sont des
/// fichiers créés par PicLib, jamais ceux de l'utilisateur. Ceux
/// de l'application en cours (même numéro de processus) sont laissés : une
/// copie peut être en train de s'écrire.
///
/// # Errors
///
/// Échoue si le dossier est illisible.
pub fn remove_partial_copies(folder: &Path) -> io::Result<usize> {
    let mut removed = 0;
    let own = format!("{PARTIAL_PREFIX}{}-", std::process::id());
    for entry in std::fs::read_dir(folder)? {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(PARTIAL_PREFIX)
            && !name.starts_with(&own)
            && name.ends_with(".part")
            && entry.file_type()?.is_file()
        {
            std::fs::remove_file(entry.path())?;
            tracing::info!(path = %entry.path().display(), "partial copy left by an interruption removed");
            removed += 1;
        }
    }
    Ok(removed)
}

/// Taille et empreinte du contenu d'un fichier, relu en entier depuis le
/// disque (pas depuis la mémoire cache).
fn digest_of(
    path: &Path,
    cancellation: &CancellationToken,
) -> Result<(u64, ContentDigest), DigestError> {
    let mut file = File::open(path).map_err(DigestError::Io)?;
    platform::disable_cache(&file).map_err(DigestError::Io)?;
    let mut hasher = Sha256::new();
    let mut buffer = vec![0_u8; CHUNK_SIZE];
    let mut size = 0_u64;
    loop {
        if cancellation.is_cancelled() {
            return Err(DigestError::Cancelled);
        }
        let read = match file.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(DigestError::Io(error)),
        };
        hasher.update(&buffer[..read]);
        size += read as u64;
    }
    Ok((size, ContentDigest(hasher.finalize().into())))
}
