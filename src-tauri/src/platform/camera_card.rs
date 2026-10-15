//! Reconnaissance des cartes mémoire et des appareils photo montés comme des
//! disques.
//!
//! Une carte (ou un appareil) se reconnaît au dossier `DCIM` à la racine de
//! son volume : c'est la norme suivie par tous les appareils photo. Un disque
//! externe ordinaire n'en a pas : il reste importable par référence.

use std::path::{Path, PathBuf};

/// Nom du dossier standard des appareils photo (« Digital Camera Images »).
const CAMERA_FOLDER: &str = "DCIM";

/// Racine du volume qui contient `path` : le dernier dossier parent situé
/// sur le même volume. `volume_of` renvoie l'identifiant du volume d'un
/// chemin existant (`None` s'il est illisible).
pub fn volume_root(path: &Path, volume_of: impl Fn(&Path) -> Option<u64>) -> Option<PathBuf> {
    let volume = volume_of(path)?;
    let mut root = path;
    for ancestor in path.ancestors().skip(1) {
        if volume_of(ancestor) != Some(volume) {
            break;
        }
        root = ancestor;
    }
    Some(root.to_path_buf())
}

/// Indique si `path` se trouve sur une carte mémoire ou un appareil photo.
pub fn is_on_camera_card(path: &Path, volume_of: impl Fn(&Path) -> Option<u64>) -> bool {
    volume_root(path, volume_of).is_some_and(|root| root.join(CAMERA_FOLDER).is_dir())
}
