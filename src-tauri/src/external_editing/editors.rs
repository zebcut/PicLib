//! Éditeurs externes : ceux que PicLib connaît,
//! repérés dans le dossier Applications, et ceux que l'utilisateur ajoute.

use std::cmp::Ordering;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

/// Éditeur connu de PicLib, avec ses habitudes : où il range ses exports,
/// quel fichier garde son travail.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Preset {
    /// DxO PhotoLab : réglages dans le `.dop` à côté de la photo, exports
    /// `photo_DxO.jpg` dans son dossier.
    DxoPhotoLab,
    /// Lightroom Classic : propose d'abord d'importer la photo dans son
    /// propre catalogue.
    LightroomClassic,
    /// darktable : réglages dans `photo.NEF.xmp`, exports dans le
    /// sous-dossier `darktable_exported`.
    Darktable,
    /// Affinity Photo : travail enregistré dans un `.afphoto`.
    AffinityPhoto,
    /// Capture One : propose d'abord d'importer la photo.
    CaptureOne,
    /// Luminar Neo : propose d'abord d'importer la photo.
    LuminarNeo,
    /// Photoshop : travail enregistré dans un PSD (ou un TIFF à calques).
    Photoshop,
}

impl Preset {
    /// Tous les éditeurs connus, dans l'ordre de la liste des préférences.
    pub const ALL: [Self; 7] = [
        Self::DxoPhotoLab,
        Self::LightroomClassic,
        Self::Darktable,
        Self::AffinityPhoto,
        Self::CaptureOne,
        Self::LuminarNeo,
        Self::Photoshop,
    ];

    /// Identifiant stable, retenu dans les réglages et le catalogue.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::DxoPhotoLab => "dxo",
            Self::LightroomClassic => "lightroom",
            Self::Darktable => "darktable",
            Self::AffinityPhoto => "affinity",
            Self::CaptureOne => "captureOne",
            Self::LuminarNeo => "luminar",
            Self::Photoshop => "photoshop",
        }
    }

    /// Nom affiché.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::DxoPhotoLab => "DxO PhotoLab",
            Self::LightroomClassic => "Lightroom Classic",
            Self::Darktable => "darktable",
            Self::AffinityPhoto => "Affinity Photo",
            Self::CaptureOne => "Capture One",
            Self::LuminarNeo => "Luminar Neo",
            Self::Photoshop => "Photoshop",
        }
    }

    /// Débuts de nom de l'application (`DxO PhotoLab 8.app`, `Adobe
    /// Photoshop 2025.app`...).
    fn application_names(self) -> &'static [&'static str] {
        match self {
            Self::DxoPhotoLab => &["DxO PhotoLab"],
            Self::LightroomClassic => &["Adobe Lightroom Classic"],
            Self::Darktable => &["darktable"],
            // « Affinity » seul : l'application réunie de 2025.
            Self::AffinityPhoto => &["Affinity Photo", "Affinity"],
            Self::CaptureOne => &["Capture One"],
            Self::LuminarNeo => &["Luminar Neo", "Luminar"],
            Self::Photoshop => &["Adobe Photoshop"],
        }
    }

    /// Sous-dossier, dans celui de la photo, où l'éditeur range ses exports
    /// par défaut.
    #[must_use]
    pub fn export_subfolder(self) -> Option<&'static str> {
        match self {
            Self::Darktable => Some("darktable_exported"),
            _ => None,
        }
    }

    /// Extensions des fichiers où l'éditeur garde son travail (calques) :
    /// « Modifier de nouveau » les rouvre plutôt que l'original
    ///.
    #[must_use]
    pub fn working_extensions(self) -> &'static [&'static str] {
        match self {
            Self::Photoshop => &["psd", "psb", "tif", "tiff"],
            Self::AffinityPhoto => &["afphoto", "af"],
            _ => &[],
        }
    }

    /// Éditeur d'identifiant `id`.
    #[must_use]
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.id() == id)
    }

    /// Rang du début de nom reconnu dans `name` (nom d'une application,
    /// sans `.app`) ; `None` si ce n'est pas cet éditeur. Ni la casse, ni
    /// les espaces ne comptent (`DXOPhotoLab9.app` est DxO PhotoLab 9), et
    /// seul un numéro de version peut suivre (`Affinity Designer` n'est pas
    /// Affinity Photo).
    fn rank(self, name: &str) -> Option<usize> {
        let name = simplified(name);
        self.application_names().iter().position(|prefix| {
            name.strip_prefix(&simplified(prefix))
                .is_some_and(|rest| rest.chars().all(|character| character.is_ascii_digit()))
        })
    }
}

/// Application ajoutée par l'utilisateur.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomEditor {
    /// Nom affiché (celui de l'application).
    pub name: String,
    /// Application (`.app`).
    pub application: PathBuf,
}

/// Réglages des éditeurs, enregistrés avec les préférences.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct EditorSettings {
    /// Applications ajoutées par l'utilisateur.
    pub custom: Vec<CustomEditor>,
    /// Éditeur de Cmd+E ; `None` : le premier de la liste.
    pub default_editor: Option<String>,
}

/// Éditeur proposé dans PicLib.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Editor {
    /// Identifiant (celui de l'éditeur connu, ou `app:` et le chemin).
    pub id: String,
    /// Nom affiché.
    pub name: String,
    /// Application à lancer.
    pub application: PathBuf,
    /// Éditeur connu ; `None` : application ajoutée par l'utilisateur.
    pub preset: Option<Preset>,
}

/// Identifiant d'une application ajoutée par l'utilisateur.
#[must_use]
pub fn custom_id(application: &Path) -> String {
    format!("app:{}", application.display())
}

/// Nom affiché de l'éditeur d'identifiant `id` (éditeur connu, ou
/// application ajoutée, même retirée depuis).
#[must_use]
pub fn editor_name(id: &str) -> String {
    match (Preset::from_id(id), id.strip_prefix("app:")) {
        (Some(preset), _) => preset.name().to_owned(),
        (None, Some(path)) => application_name(Path::new(path)),
        (None, None) => id.to_owned(),
    }
}

/// Nom affiché d'une application : son nom de fichier sans `.app`.
#[must_use]
pub fn application_name(application: &Path) -> String {
    application
        .file_stem()
        .map_or_else(String::new, |name| name.to_string_lossy().into_owned())
}

/// Éditeurs proposés : les éditeurs connus trouvés dans `application_dirs`
/// (la version la plus récente de chacun), puis les applications ajoutées
/// (une application déjà proposée n'est pas répétée).
#[must_use]
pub fn available(settings: &EditorSettings, application_dirs: &[PathBuf]) -> Vec<Editor> {
    let applications = find_applications(application_dirs);
    let mut editors: Vec<Editor> = Preset::ALL
        .into_iter()
        .filter_map(|preset| {
            applications
                .iter()
                .filter_map(|application| {
                    let name = application_name(application);
                    preset.rank(&name).map(|rank| (rank, name, application))
                })
                // Affinity Photo avant « Affinity », Luminar Neo avant
                // Luminar ; puis la version la plus récente.
                .min_by(|(rank_a, name_a, _), (rank_b, name_b, _)| {
                    rank_a
                        .cmp(rank_b)
                        .then_with(|| natural_order(&simplified(name_b), &simplified(name_a)))
                })
                .map(|(_, _, application)| application)
                .map(|application| Editor {
                    id: preset.id().to_owned(),
                    name: preset.name().to_owned(),
                    application: application.clone(),
                    preset: Some(preset),
                })
        })
        .collect();
    for custom in &settings.custom {
        if editors
            .iter()
            .any(|editor| editor.application == custom.application)
        {
            continue;
        }
        editors.push(Editor {
            id: custom_id(&custom.application),
            name: custom.name.clone(),
            application: custom.application.clone(),
            preset: None,
        });
    }
    editors
}

/// Éditeur de Cmd+E : celui choisi s'il est proposé, sinon le premier.
#[must_use]
pub fn default_editor<'a>(editors: &'a [Editor], settings: &EditorSettings) -> Option<&'a Editor> {
    settings
        .default_editor
        .as_deref()
        .and_then(|id| editors.iter().find(|editor| editor.id == id))
        .or_else(|| editors.first())
}

/// Applications (`.app`) de `dirs` et de leurs sous-dossiers directs (Adobe
/// range les siennes dans un dossier à leur nom).
fn find_applications(dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if is_application(&path) {
                found.push(path);
            } else if path.is_dir()
                && let Ok(inner) = std::fs::read_dir(&path)
            {
                found.extend(
                    inner
                        .flatten()
                        .map(|entry| entry.path())
                        .filter(|path| is_application(path)),
                );
            }
        }
    }
    found
}

/// Nom en minuscules, sans espaces ni ponctuation.
fn simplified(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|character| character.to_ascii_lowercase())
        .collect()
}

fn is_application(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "app") && path.is_dir()
}

/// Ordre « naturel » de deux noms : les nombres sont comparés comme des
/// nombres (`DxO PhotoLab 10` après `DxO PhotoLab 9`).
fn natural_order(a: &str, b: &str) -> Ordering {
    let key = |name: &str| -> Vec<(u64, String)> {
        let mut parts = Vec::new();
        let mut digits = String::new();
        let mut text = String::new();
        for character in name.chars() {
            if character.is_ascii_digit() {
                if !text.is_empty() {
                    parts.push((0, std::mem::take(&mut text)));
                }
                digits.push(character);
            } else {
                if !digits.is_empty() {
                    parts.push((digits.parse().unwrap_or(u64::MAX), String::new()));
                    digits.clear();
                }
                text.push(character);
            }
        }
        if !digits.is_empty() {
            parts.push((digits.parse().unwrap_or(u64::MAX), String::new()));
        }
        if !text.is_empty() {
            parts.push((0, text));
        }
        parts
    };
    key(a).cmp(&key(b))
}
