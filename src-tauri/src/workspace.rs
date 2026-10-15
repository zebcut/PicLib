//! Dossier de travail de PicLib.
//!
//! Tout ce qui dépend du catalogue est rangé dans un seul dossier, choisi par
//! l'utilisateur (par défaut dans ses Documents, voir [`crate::platform`]) :
//!
//! ```text
//! PicLib.nosync/
//!   Catalogue.sqlite    la base du catalogue
//!   Backups/            copies du catalogue
//!   Thumbnails/         vignettes des photos et vidéos
//!   Previews/           aperçus de la Loupe (cache limité en taille)
//! ```
//!
//! Les noms sont fixes et ne suivent pas la langue de l'interface : un
//! changement de langue ne doit jamais « perdre » le catalogue. Ils sont en
//! anglais depuis la version publique ; un dossier de travail créé avant
//! garde ses dossiers aux noms français, s'ils existent.

use std::path::{Path, PathBuf};

/// Nom du fichier du catalogue dans le dossier de travail.
const CATALOG_FILE_NAME: &str = "Catalogue.sqlite";

/// Nom du dossier des sauvegardes du catalogue, et son ancien nom.
const BACKUPS_DIR_NAME: (&str, &str) = ("Backups", "Sauvegardes");

/// Nom du dossier du cache des vignettes, et son ancien nom.
const THUMBNAILS_DIR_NAME: (&str, &str) = ("Thumbnails", "Vignettes");

/// Nom du dossier du cache des aperçus, et son ancien nom.
const PREVIEWS_DIR_NAME: (&str, &str) = ("Previews", "Aperçus");

/// Nom du dossier du cache temporaire des vignettes de l'écran d'import, et
/// son ancien nom.
const IMPORT_PREVIEWS_DIR_NAME: (&str, &str) = ("Import thumbnails", "Vignettes d'import");

/// Nom du dossier des copies des fichiers XMP d'autres logiciels, gardées
/// avant leur première modification par PicLib, et son ancien nom.
const XMP_ORIGINALS_DIR_NAME: (&str, &str) = ("Original XMP", "XMP d'origine");

/// Destination par défaut de l'import par copie :
/// les photos restent à part des fichiers techniques du catalogue.
const MEDIA_DIR_NAME: &str = "Media";

/// Emplacement du dossier de travail et de son contenu.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Workspace {
    root: PathBuf,
}

impl Workspace {
    /// Dossier de travail situé à `root`.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }

    /// Dossier de travail lui-même.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Fichier du catalogue.
    #[must_use]
    pub fn catalog_path(&self) -> PathBuf {
        self.root.join(CATALOG_FILE_NAME)
    }

    /// Dossier des sauvegardes du catalogue.
    #[must_use]
    pub fn backups_dir(&self) -> PathBuf {
        self.dir(BACKUPS_DIR_NAME)
    }

    /// Dossier du cache des vignettes.
    #[must_use]
    pub fn thumbnails_dir(&self) -> PathBuf {
        self.dir(THUMBNAILS_DIR_NAME)
    }

    /// Dossier du cache des aperçus.
    #[must_use]
    pub fn previews_dir(&self) -> PathBuf {
        self.dir(PREVIEWS_DIR_NAME)
    }

    /// Dossier du cache temporaire des vignettes de l'écran d'import.
    #[must_use]
    pub fn import_previews_dir(&self) -> PathBuf {
        self.dir(IMPORT_PREVIEWS_DIR_NAME)
    }

    /// Dossier des copies des fichiers XMP d'autres logiciels, un
    /// sous-dossier par jour d'écriture (créé à la première copie).
    #[must_use]
    pub fn xmp_originals_dir(&self) -> PathBuf {
        self.dir(XMP_ORIGINALS_DIR_NAME)
    }

    /// Dossiers internes du catalogue (vignettes, aperçus, sauvegardes) : ils
    /// ne contiennent pas de photos de l'utilisateur et ne sont jamais
    /// proposés à l'import.
    #[must_use]
    pub fn internal_dirs(&self) -> Vec<PathBuf> {
        vec![
            self.thumbnails_dir(),
            self.previews_dir(),
            self.import_previews_dir(),
            self.backups_dir(),
            self.xmp_originals_dir(),
        ]
    }

    /// Dossier `names.0`, ou `names.1` (son ancien nom français) s'il existe
    /// déjà : un dossier de travail créé avant garde ses dossiers.
    fn dir(&self, (name, former): (&str, &str)) -> PathBuf {
        let former = self.root.join(former);
        if former.is_dir() {
            former
        } else {
            self.root.join(name)
        }
    }

    /// Destination par défaut de l'import par copie (créée à la première
    /// copie).
    #[must_use]
    pub fn media_dir(&self) -> PathBuf {
        self.root.join(MEDIA_DIR_NAME)
    }
}
