//! Disque qui contient un chemin, pour regrouper les dossiers du catalogue
//! par disque dans le panneau Dossiers, même quand le disque
//! n'est pas branché.
//!
//! Sur macOS, les autres disques sont montés sous `/Volumes/<nom>` ; tout le
//! reste est sur le disque de démarrage, dont `/Volumes` contient aussi un
//! lien vers `/`. Le chemin suffit donc à reconnaître le disque, sans le
//! lire.

use std::path::{Component, Path, PathBuf};

/// Disque qui contient un dossier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Disk {
    /// Nom du disque ; `None` pour le disque de démarrage si son nom est
    /// inconnu.
    pub name: Option<String>,
    /// Racine du disque (`/` ou `/Volumes/<nom>`).
    pub root: PathBuf,
}

/// Disque qui contient `path`. `volumes_dir` est le dossier où les disques
/// sont montés (`/Volumes`) ; `startup` est le disque de démarrage.
#[must_use]
pub fn disk_of(path: &Path, volumes_dir: &Path, startup: &Disk) -> Disk {
    let Ok(rest) = path.strip_prefix(volumes_dir) else {
        return startup.clone();
    };
    match rest.components().next() {
        Some(Component::Normal(name)) => {
            let name = name.to_string_lossy().into_owned();
            // Le disque de démarrage figure aussi sous `/Volumes`.
            if startup.name.as_deref() == Some(name.as_str()) {
                return startup.clone();
            }
            Disk {
                root: volumes_dir.join(&name),
                name: Some(name),
            }
        }
        _ => startup.clone(),
    }
}

/// Disque de démarrage : son nom est celui du lien de `volumes_dir` qui
/// mène à `system_root` (« Macintosh HD » → `/`).
#[must_use]
pub fn startup_disk(volumes_dir: &Path, system_root: &Path) -> Disk {
    let name = std::fs::read_dir(volumes_dir).ok().and_then(|entries| {
        entries.flatten().find_map(|entry| {
            let target = std::fs::read_link(entry.path()).ok()?;
            (target == system_root).then(|| entry.file_name().to_string_lossy().into_owned())
        })
    });
    Disk {
        name,
        root: system_root.to_path_buf(),
    }
}

/// Disque monté, proposé comme source d'import.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountedVolume {
    /// Nom du disque.
    pub name: String,
    /// Racine du disque (`/Volumes/<nom>`).
    pub root: PathBuf,
}

/// Disques montés sous `volumes_dir`, par nom : ni le lien vers le disque
/// de démarrage, ni les volumes cachés ou du système (instantanés de Time
/// Machine).
#[must_use]
pub fn mounted_volumes(volumes_dir: &Path) -> Vec<MountedVolume> {
    let Ok(entries) = std::fs::read_dir(volumes_dir) else {
        return Vec::new();
    };
    let mut volumes: Vec<MountedVolume> = entries
        .flatten()
        .filter(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            (!name.starts_with('.') && !name.starts_with("com.apple.")).then(|| MountedVolume {
                root: entry.path(),
                name,
            })
        })
        .collect();
    volumes.sort_by(|a, b| a.name.cmp(&b.name));
    volumes
}

/// Indique si un disque est branché : sa racine existe.
#[must_use]
pub fn is_online(disk: &Disk) -> bool {
    disk.root.is_dir()
}
