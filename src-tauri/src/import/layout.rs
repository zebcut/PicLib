//! Rangement des copies dans la destination : par
//! date d'après un modèle, dans un seul dossier, ou en conservant
//! l'arborescence de la source ; avec, au choix, un sous-dossier de séance
//! au dernier niveau.

use std::collections::BTreeMap;
use std::path::{Component, Path, PathBuf};

use serde::{Deserialize, Serialize};
use specta::Type;

use crate::domain::CaptureTime;

/// Élément d'un modèle de rangement par date.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DatePart {
    /// Année, quatre chiffres.
    Year,
    /// Mois, deux chiffres.
    Month,
    /// Nom du mois, dans la langue de l'interface.
    MonthName,
    /// Jour, deux chiffres.
    Day,
    /// Texte libre.
    Text {
        /// Texte.
        text: String,
    },
    /// « / » : un niveau de dossier de plus.
    Level,
}

/// Organisation des copies dans la destination.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum DestinationLayout {
    /// Par date de prise de vue, d'après un modèle.
    Date {
        /// Éléments, dans l'ordre.
        parts: Vec<DatePart>,
    },
    /// Dans un seul dossier.
    SingleFolder {
        /// Nom du dossier (vide : la destination elle-même).
        name: String,
    },
    /// En conservant l'arborescence de la source.
    SourceTree,
}

impl Default for DestinationLayout {
    /// `AAAA/MM/JJ`.
    fn default() -> Self {
        Self::Date {
            parts: vec![
                DatePart::Year,
                DatePart::Level,
                DatePart::Month,
                DatePart::Level,
                DatePart::Day,
            ],
        }
    }
}

/// Rangement d'un import : l'organisation, les noms des mois et le
/// sous-dossier de séance.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Arrangement {
    /// Organisation.
    pub layout: DestinationLayout,
    /// Noms des douze mois, dans la langue de l'interface ; à défaut, le
    /// numéro du mois.
    pub month_names: Vec<String>,
    /// Sous-dossier ajouté au dernier niveau (ex. « Mariage Paul »).
    pub session: Option<String>,
}

impl Arrangement {
    /// Dossier, relatif à la destination, où copier un fichier de la source
    /// `source_root` pris à `capture_time`.
    #[must_use]
    pub fn relative_folder(
        &self,
        source_root: &Path,
        file: &Path,
        capture_time: &CaptureTime,
    ) -> PathBuf {
        let mut folder = match &self.layout {
            DestinationLayout::Date { parts } => self.date_levels(parts, capture_time),
            DestinationLayout::SingleFolder { name } => clean_level(name).into_iter().collect(),
            DestinationLayout::SourceTree => file
                .parent()
                .and_then(|parent| parent.strip_prefix(source_root).ok())
                .map(|relative| {
                    relative
                        .components()
                        .filter_map(|component| match component {
                            Component::Normal(name) => Some(name),
                            _ => None,
                        })
                        .collect()
                })
                .unwrap_or_default(),
        };
        if let Some(session) = self.session.as_deref().and_then(clean_level) {
            folder.push(session);
        }
        folder
    }

    fn date_levels(&self, parts: &[DatePart], capture_time: &CaptureTime) -> PathBuf {
        let local = capture_time.local;
        let mut levels = vec![String::new()];
        for part in parts {
            let text = match part {
                DatePart::Year => format!("{:04}", local.year()),
                DatePart::Month => format!("{:02}", local.month()),
                DatePart::MonthName => {
                    let index = usize::try_from(local.month() - 1).unwrap_or(0);
                    self.month_names
                        .get(index)
                        .filter(|name| !name.trim().is_empty())
                        .cloned()
                        .unwrap_or_else(|| format!("{:02}", local.month()))
                }
                DatePart::Day => format!("{:02}", local.day()),
                DatePart::Text { text } => text.clone(),
                DatePart::Level => {
                    levels.push(String::new());
                    continue;
                }
            };
            if let Some(level) = levels.last_mut() {
                level.push_str(&text);
            }
        }
        levels
            .iter()
            .filter_map(|level| clean_level(level))
            .collect()
    }
}

/// Nom de dossier sûr : les caractères interdits (« / », « \ », « : »,
/// caractères de contrôle) deviennent « - », sans espaces ni points au
/// début (pas de dossier caché, pas de « .. ») ; `None` s'il est vide.
fn clean_level(text: &str) -> Option<String> {
    let cleaned: String = text
        .chars()
        .map(|character| match character {
            '/' | '\\' | ':' => '-',
            other if other.is_control() => '-',
            other => other,
        })
        .collect();
    let cleaned = cleaned.trim().trim_start_matches('.').trim();
    (!cleaned.is_empty()).then(|| cleaned.to_owned())
}

/// Dossier de l'aperçu de l'arborescence.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PlannedFolder {
    /// Chemin relatif à la destination.
    pub path: PathBuf,
    /// Photos et vidéos copiées directement dans ce dossier.
    pub file_count: u64,
    /// Vrai si le dossier n'existe pas encore.
    pub is_new: bool,
}

/// Aperçu de l'arborescence qui sera créée dans `destination` :
/// chaque dossier, ses parents compris, avec le nombre de fichiers qui y
/// seront copiés ; `folders` donne le dossier relatif de chaque fichier.
#[must_use]
pub fn planned_tree(destination: &Path, folders: &[PathBuf]) -> Vec<PlannedFolder> {
    let mut counts: BTreeMap<PathBuf, u64> = BTreeMap::new();
    for folder in folders {
        let mut ancestor = PathBuf::new();
        for component in folder.components() {
            ancestor.push(component);
            counts.entry(ancestor.clone()).or_insert(0);
        }
        *counts.entry(folder.clone()).or_insert(0) += 1;
    }
    counts
        .into_iter()
        .map(|(path, file_count)| PlannedFolder {
            is_new: !destination.join(&path).is_dir(),
            path,
            file_count,
        })
        .collect()
}
