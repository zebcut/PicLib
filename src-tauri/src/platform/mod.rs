//! Comportements propres à chaque système d'exploitation.
//!
//! Aucune API ni convention spécifique à macOS ou à Windows ne doit apparaître
//! ailleurs que dans ce module.

use std::path::{Path, PathBuf};

use crate::domain::MediaFormat;

mod camera_card;
mod disks;
#[cfg(target_os = "macos")]
mod macos;
mod sync_detection;

pub use disks::{Disk, MountedVolume, is_online as is_disk_online};

/// Moteur qui décode et réduit les photos.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageEngine {
    /// Le processeur (ImageIO sur macOS).
    Cpu,
    /// Le GPU (Core Image sur macOS) ; décode entièrement les RAW.
    Gpu,
}

/// Vignette à fabriquer.
#[derive(Debug, Clone, Copy)]
pub struct ThumbnailRequest<'a> {
    /// Photo d'origine (lue, jamais modifiée).
    pub source: &'a Path,
    /// Fichier JPEG à créer ; il ne doit pas exister.
    pub destination: &'a Path,
    /// Taille maximale du plus grand côté, en pixels.
    pub max_pixel_size: u32,
    /// Qualité JPEG, de 0 à 1.
    pub jpeg_quality: f64,
    /// Utiliser en priorité l'aperçu intégré au fichier : vrai pour un RAW
    ///, et seulement pour lui.
    pub prefer_embedded_preview: bool,
    /// Moteur qui décode la photo si l'aperçu intégré ne convient pas (voir
    /// [`preferred_engine`]) ; sans effet pour une vidéo.
    pub engine: ImageEngine,
}

/// Vignette fabriquée.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThumbnailInfo {
    /// Largeur de la vignette, en pixels, orientation appliquée.
    pub width: u32,
    /// Hauteur de la vignette, en pixels, orientation appliquée.
    pub height: u32,
    /// Largeur de la photo d'origine, si elle est connue.
    pub source_width: Option<u32>,
    /// Hauteur de la photo d'origine, si elle est connue.
    pub source_height: Option<u32>,
    /// Vrai si l'aperçu intégré au fichier a été utilisé.
    pub used_embedded_preview: bool,
}

/// Photo entière à fabriquer en JPEG, en mémoire (affichage à 100 % d'un
/// format que l'interface ne sait pas lire, comme un RAW).
#[derive(Debug, Clone, Copy)]
pub struct FullSizeRequest<'a> {
    /// Photo d'origine (lue, jamais modifiée).
    pub source: &'a Path,
    /// Utiliser en priorité l'aperçu intégré au fichier : vrai pour un RAW
    ///, et seulement pour lui.
    pub prefer_embedded_preview: bool,
    /// En dessous de cette taille (plus grand côté, en pixels), l'aperçu
    /// intégré est jugé trop petit et la photo est décodée.
    pub min_embedded_preview_size: u32,
    /// Qualité JPEG, de 0 à 1.
    pub jpeg_quality: f64,
    /// Moteur qui décode la photo si l'aperçu intégré ne convient pas.
    pub engine: ImageEngine,
}

/// Photo entière encodée en JPEG.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FullSizeImage {
    /// Contenu du fichier JPEG.
    pub jpeg: Vec<u8>,
    /// Largeur, en pixels, orientation appliquée.
    pub width: u32,
    /// Hauteur, en pixels, orientation appliquée.
    pub height: u32,
}

/// Erreur de fabrication d'une vignette.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ThumbnailError {
    /// Le chemin ne peut pas être transmis au système.
    #[error("invalid path")]
    InvalidPath,
    /// Le fichier n'a pas pu être décodé (format inconnu du système, fichier
    /// abîmé ou introuvable).
    #[error("the image cannot be decoded")]
    Undecodable,
    /// La vignette n'a pas pu être écrite.
    #[error("the thumbnail cannot be written")]
    WriteFailed,
    /// Pas de décodeur pour ce système (Windows).
    #[error("thumbnails are not supported on this system yet")]
    Unsupported,
}

/// Fabrique la vignette JPEG d'une vidéo (image du milieu) avec le moteur
/// vidéo du système.
///
/// # Errors
///
/// Voir [`ThumbnailError`].
pub fn make_video_thumbnail(
    request: &ThumbnailRequest<'_>,
) -> Result<ThumbnailInfo, ThumbnailError> {
    #[cfg(target_os = "macos")]
    {
        macos::av_foundation::make_thumbnail(request)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        Err(ThumbnailError::Unsupported)
    }
}

/// Fabrique une vignette JPEG avec le décodeur d'images du système, sur le
/// processeur ou le GPU selon `request.engine` (repli sur le processeur si le
/// GPU échoue ou n'est pas utilisable).
///
/// # Errors
///
/// Voir [`ThumbnailError`].
pub fn make_thumbnail(request: &ThumbnailRequest<'_>) -> Result<ThumbnailInfo, ThumbnailError> {
    #[cfg(target_os = "macos")]
    {
        macos::image_io::make_thumbnail(request)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        Err(ThumbnailError::Unsupported)
    }
}

/// Nom de l'objectif d'une photo, lu par le décodeur d'images du système,
/// qui connaît aussi les données propres aux fabricants ; `None` si rien
/// n'est connu (ou hors de macOS, pour l'instant).
#[must_use]
pub fn lens_model(path: &Path) -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        macos::image_io::lens_model(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

/// Fabrique la photo entière en JPEG, en mémoire, avec le décodeur d'images
/// du système, sur le processeur ou le GPU selon `request.engine`.
///
/// # Errors
///
/// Voir [`ThumbnailError`].
pub fn make_full_size_jpeg(request: &FullSizeRequest<'_>) -> Result<FullSizeImage, ThumbnailError> {
    #[cfg(target_os = "macos")]
    {
        macos::image_io::make_full_size_jpeg(request)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = request;
        Err(ThumbnailError::Unsupported)
    }
}

/// Erreur de mise à la corbeille.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TrashError {
    /// Le chemin ne peut pas être transmis au système.
    #[error("invalid path")]
    InvalidPath,
    /// Le système a refusé (droits, volume sans corbeille...).
    #[error("{0}")]
    System(String),
    /// Pas encore pris en charge sur ce système (Windows).
    #[error("moving to the trash is not supported on this system yet")]
    Unsupported,
}

/// Place un fichier dans la corbeille du système (jamais d'effacement
/// définitif) et renvoie son nouvel emplacement, s'il est connu. À n'appeler
/// qu'à travers le module `files`.
///
/// # Errors
///
/// Voir [`TrashError`] : le fichier reste alors à sa place.
pub fn move_to_trash(path: &Path) -> Result<Option<PathBuf>, TrashError> {
    #[cfg(target_os = "macos")]
    {
        macos::trash::move_to_trash(path)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(TrashError::Unsupported)
    }
}

/// Renomme ou déplace `from` en `to` sur le même volume, sans jamais
/// remplacer ce qui existe à `to` : `ErrorKind::AlreadyExists` dans ce cas.
/// Sur macOS, un renommage ordinaire remplacerait sans prévenir un dossier
/// vide de même nom ; la variante exclusive du système l'interdit, sans
/// laisser d'instant entre la vérification et le renommage. À n'appeler
/// qu'à travers le module `files`.
///
/// # Errors
///
/// Renvoie l'erreur du système : `from` reste alors à sa place.
pub fn rename_exclusive(from: &Path, to: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::rename::rename_exclusive(from, to)
    }
    // À compléter pour Windows : `MoveFileExW` sans
    // `MOVEFILE_REPLACE_EXISTING` refuse déjà de remplacer.
    #[cfg(not(target_os = "macos"))]
    {
        if to.symlink_metadata().is_ok() {
            return Err(std::io::ErrorKind::AlreadyExists.into());
        }
        std::fs::rename(from, to)
    }
}

/// Indique si deux chemins existants sont sur le même volume (disque,
/// partition) : un déplacement n'y est qu'un renommage, instantané.
#[must_use]
pub fn same_volume(a: &Path, b: &Path) -> bool {
    matches!((volume_id(a), volume_id(b)), (Some(a), Some(b)) if a == b)
}

/// Demande au système de ne pas garder en mémoire cache le contenu lu ou
/// écrit par `file` : une copie relue l'est alors depuis le disque. Sans effet ailleurs que sur macOS pour l'instant (Windows). À n'appeler qu'à travers le module `files`.
///
/// # Errors
///
/// Renvoie l'erreur du système si le réglage est refusé.
pub fn disable_cache(file: &std::fs::File) -> std::io::Result<()> {
    #[cfg(target_os = "macos")]
    {
        macos::uncached::disable_cache(file)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = file;
        Ok(())
    }
}

/// Indique si le GPU est utilisable pour traiter les images.
#[must_use]
pub fn gpu_available() -> bool {
    #[cfg(target_os = "macos")]
    {
        macos::core_image::is_available()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Usage de l'image à fabriquer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImagePurpose {
    /// Vignette de la grille (512 pixels), fabriquée en masse.
    Thumbnail,
    /// Aperçu de la Loupe (2560 pixels), une photo à la fois.
    Preview,
    /// Photo entière, pour le zoom à 100 %.
    FullSize,
}

/// Moteur le plus rapide pour un format et un usage, d'après les mesures
/// faites sur un Mac M2 :
///
/// - HEIC : GPU (trois fois plus rapide, en masse comme à l'unité) ;
/// - JPEG : GPU pour un aperçu isolé (deux fois plus rapide), processeur pour
///   les vignettes en masse (plus rapide avec plusieurs fils) ;
/// - RAW : GPU pour la photo entière (jusqu'à trois fois plus rapide),
///   processeur pour les tailles réduites (le GPU y est bien plus lent) ;
/// - PNG, TIFF : processeur (non mesurés : pas de changement sans mesure).
#[must_use]
pub fn preferred_engine(format: MediaFormat, purpose: ImagePurpose) -> ImageEngine {
    let gpu_is_faster = match format {
        MediaFormat::Heic => true,
        MediaFormat::Jpeg => purpose == ImagePurpose::Preview,
        raw if raw.is_raw() => purpose == ImagePurpose::FullSize,
        _ => false,
    };
    if gpu_is_faster && gpu_available() {
        ImageEngine::Gpu
    } else {
        ImageEngine::Cpu
    }
}

/// Nom du dossier de travail créé par défaut dans les Documents.
///
/// Sur macOS, le dossier Documents est souvent synchronisé par iCloud Drive,
/// ce qui peut corrompre une base SQLite ouverte et évacuer les fichiers du
/// disque. iCloud ignore les éléments dont le nom se termine par `.nosync`.
#[cfg(target_os = "macos")]
const DEFAULT_WORKSPACE_DIR_NAME: &str = "PicLib.nosync";

/// Nom du dossier de travail créé par défaut dans les Documents.
///
/// À revoir pour Windows : le dossier Documents y est souvent
/// redirigé vers OneDrive, que `.nosync` ne concerne pas.
#[cfg(not(target_os = "macos"))]
const DEFAULT_WORKSPACE_DIR_NAME: &str = "PicLib";

/// Dossier de travail proposé par défaut, dans le dossier Documents de
/// l'utilisateur.
#[must_use]
pub fn default_workspace_dir(documents_dir: &Path) -> PathBuf {
    documents_dir.join(DEFAULT_WORKSPACE_DIR_NAME)
}

/// Indique si `path` se trouve sur une carte mémoire ou un appareil photo
/// (volume avec un dossier `DCIM` à sa racine). Ces sources ne
/// s'importent que par copie, car la carte sera formatée.
#[must_use]
pub fn is_on_camera_card(path: &Path) -> bool {
    camera_card::is_on_camera_card(path, volume_id)
}

/// Racine de la carte mémoire (ou de l'appareil photo) qui contient `path`,
/// s'il est sur une carte.
#[must_use]
pub fn camera_card_root(path: &Path) -> Option<PathBuf> {
    camera_card::volume_root(path, volume_id).filter(|root| is_on_camera_card(root))
}

/// Échec de l'éjection d'un disque.
#[derive(Debug, thiserror::Error)]
pub enum EjectError {
    /// Le système a refusé (un programme utilise encore le disque).
    #[error("the disk could not be ejected: {0}")]
    Refused(String),
    /// L'outil du système n'a pas pu être lancé.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Éjecte le disque monté à `root` (une carte mémoire, après sa copie) :
/// il peut ensuite être retiré sans risque. Rien n'est écrit sur le disque.
///
/// # Errors
///
/// Voir [`EjectError`].
pub fn eject(root: &Path) -> Result<(), EjectError> {
    #[cfg(target_os = "macos")]
    {
        // `diskutil` est l'outil du système pour démonter et éjecter.
        let output = std::process::Command::new("/usr/sbin/diskutil")
            .arg("eject")
            .arg(root)
            .output()?;
        if output.status.success() {
            Ok(())
        } else {
            Err(EjectError::Refused(
                String::from_utf8_lossy(&output.stderr).trim().to_owned(),
            ))
        }
    }
    // À compléter pour Windows.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = root;
        Err(EjectError::Refused("not supported".to_owned()))
    }
}

/// Dossiers où sont installées les applications (`home` : dossier de
/// l'utilisateur), où chercher les éditeurs externes.
#[must_use]
pub fn application_dirs(home: Option<&Path>) -> Vec<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        std::iter::once(PathBuf::from("/Applications"))
            .chain(home.map(|home| home.join("Applications")))
            .collect()
    }
    // À compléter pour Windows : dossiers Program Files.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = home;
        Vec::new()
    }
}

/// Échec de l'ouverture de fichiers dans une autre application.
#[derive(Debug, thiserror::Error)]
pub enum OpenError {
    /// Le système a refusé (application introuvable ou abîmée...).
    #[error("the system refused to open the files: {0}")]
    Refused(String),
    /// L'outil du système n'a pas pu être lancé.
    #[error(transparent)]
    Io(#[from] std::io::Error),
    /// Pas encore pris en charge sur ce système (Windows).
    #[error("opening files in another application is not supported on this system yet")]
    Unsupported,
}

/// Ouvre les fichiers `files` dans l'application `application` (un éditeur externe), qui est lancée au besoin. PicLib
/// n'attend pas la fin de la retouche.
///
/// # Errors
///
/// Voir [`OpenError`].
pub fn open_with_application(application: &Path, files: &[PathBuf]) -> Result<(), OpenError> {
    // Un chemin relatif pourrait commencer par « - » et être lu comme une
    // option de l'outil : seuls des chemins absolus lui sont passés.
    if !application.is_absolute() || files.iter().any(|file| !file.is_absolute()) {
        return Err(OpenError::Refused("relative path".to_owned()));
    }
    #[cfg(target_os = "macos")]
    {
        // `open -a` est l'outil du système : il lance l'application si elle
        // ne l'est pas, et lui confie les fichiers.
        let mut command = std::process::Command::new("/usr/bin/open");
        command.arg("-a").arg(application);
        command.args(files);
        run_system_tool(&mut command)
    }
    // À compléter pour Windows.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = (application, files);
        Err(OpenError::Unsupported)
    }
}

/// Montre le fichier `path` dans le Finder, sélectionné (« Afficher dans le Finder »).
///
/// # Errors
///
/// Voir [`OpenError`].
pub fn reveal_in_file_manager(path: &Path) -> Result<(), OpenError> {
    if !path.is_absolute() {
        return Err(OpenError::Refused("relative path".to_owned()));
    }
    #[cfg(target_os = "macos")]
    {
        let mut command = std::process::Command::new("/usr/bin/open");
        command.arg("-R").arg(path);
        run_system_tool(&mut command)
    }
    // À compléter pour Windows : `explorer /select,`.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        Err(OpenError::Unsupported)
    }
}

/// Lance un outil du système et attend sa réponse.
#[cfg(target_os = "macos")]
fn run_system_tool(command: &mut std::process::Command) -> Result<(), OpenError> {
    let output = command.output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(OpenError::Refused(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ))
    }
}

/// Dossier où macOS monte les autres disques.
const VOLUMES_DIR: &str = "/Volumes";

/// Disque de démarrage (nom lu dans `/Volumes`), à demander une fois par
/// lecture du panneau Dossiers.
#[must_use]
pub fn startup_disk() -> Disk {
    #[cfg(target_os = "macos")]
    {
        disks::startup_disk(Path::new(VOLUMES_DIR), Path::new("/"))
    }
    // À compléter pour Windows : un disque par lettre de lecteur.
    #[cfg(not(target_os = "macos"))]
    {
        Disk {
            name: None,
            root: PathBuf::from("/"),
        }
    }
}

/// Disque qui contient `path`, reconnu à son chemin : il n'a pas besoin
/// d'être branché.
#[must_use]
pub fn disk_of(path: &Path, startup: &Disk) -> Disk {
    disks::disk_of(path, Path::new(VOLUMES_DIR), startup)
}

/// Disques montés en plus du disque de démarrage (disques externes,
/// cartes mémoire, appareils photo), proposés comme sources d'import.
#[must_use]
pub fn mounted_volumes() -> Vec<MountedVolume> {
    #[cfg(target_os = "macos")]
    {
        disks::mounted_volumes(Path::new(VOLUMES_DIR))
    }
    // À compléter pour Windows : un disque par lettre de lecteur.
    #[cfg(not(target_os = "macos"))]
    {
        Vec::new()
    }
}

/// Identifiant du volume (disque, partition, carte) qui contient `path`.
#[cfg(unix)]
fn volume_id(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).ok().map(|metadata| metadata.dev())
}

/// Identifiant du volume qui contient `path`. À compléter pour Windows
/// : lettre de lecteur.
#[cfg(not(unix))]
fn volume_id(_path: &Path) -> Option<u64> {
    None
}

/// Indique si `path` (existant ou non) se trouve dans un dossier synchronisé
/// avec un service en ligne (iCloud Drive, OneDrive, Dropbox, kDrive...).
///
/// Un dossier dont le nom se termine par `.nosync` n'est pas synchronisé par
/// iCloud. La détection est une aide pour avertir l'utilisateur : elle peut
/// manquer un service rare, jamais bloquer une action.
#[must_use]
pub fn is_synced_location(path: &Path) -> bool {
    #[cfg(target_os = "macos")]
    {
        sync_detection::is_synced_location(path, macos::is_file_provider_root)
    }
    // À compléter pour Windows : OneDrive, Dropbox...
    #[cfg(not(target_os = "macos"))]
    {
        sync_detection::is_synced_location(path, |_| false)
    }
}
