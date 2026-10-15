//! Choix de noms qui n'écrasent jamais un fichier existant.

use std::path::{Path, PathBuf};

/// Premier chemin libre parmi `stem.ext`, `stem-2.ext`, `stem-3.ext`...
///
/// Un fichier ou dossier existant n'est ainsi jamais écrasé. Le choix reste
/// indicatif si un autre programme crée le même nom entre-temps : l'opération
/// qui suit doit elle-même refuser d'écraser (création exclusive).
#[must_use]
pub fn unique_path(dir: &Path, stem: &str, extension: &str) -> PathBuf {
    let mut candidate = dir.join(format!("{stem}.{extension}"));
    let mut suffix = 2_u32;
    while candidate.exists() {
        candidate = dir.join(format!("{stem}-{suffix}.{extension}"));
        suffix += 1;
    }
    candidate
}

/// Premier nom de base libre parmi `stem`, `stem-2`, `stem-3`... pour tout
/// un groupe de fichiers qui doivent le partager : chaque nom du groupe est
/// ce nom de base suivi d'une des `endings` (`.NEF`, `.JPG`, `.NEF.xmp`...).
/// Une paire RAW + JPEG et ses fichiers annexes gardent ainsi le même nom.
///
/// Même réserve que [`unique_path`] : le nommage lui-même doit refuser
/// d'écraser.
#[must_use]
pub fn unique_stem(dir: &Path, stem: &str, endings: &[String]) -> String {
    let is_free = |candidate: &str| {
        endings.iter().all(|ending| {
            dir.join(format!("{candidate}{ending}"))
                .symlink_metadata()
                .is_err()
        })
    };
    if is_free(stem) {
        return stem.to_owned();
    }
    (2..=u32::MAX)
        .map(|suffix| format!("{stem}-{suffix}"))
        .find(|candidate| is_free(candidate))
        .unwrap_or_else(|| stem.to_owned())
}
