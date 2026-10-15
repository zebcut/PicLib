//! Empilement automatique : les paires RAW + JPEG et les
//! rafales d'un import, d'un dossier ou d'une sélection. Pour les paires,
//! une photo déjà dans une pile n'est pas touchée ; pour les rafales, une
//! pile existante compte comme une seule photo et rejoint la rafale.

use std::collections::{BTreeMap, HashSet};
use std::path::Path;

use rusqlite::Connection;

use super::{StackError, perform};
use crate::catalog::repositories::stacks::{self, AutoCandidate, CandidateScope, Stack};
use crate::catalog::{Catalog, CatalogError};
use crate::domain::{FolderId, ImportId, MediaFormat, MediaId};
use crate::edits::{EditSummary, History, StackChange};

/// Photos à examiner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoScope {
    /// Ces photos ; pour les paires, avec leur RAW ou leur JPEG.
    Media(Vec<MediaId>),
    /// Un dossier et ses sous-dossiers.
    Folder(FolderId),
}

/// Façon d'empiler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AutoMethod {
    /// Les paires RAW + JPEG : même nom, même dossier, le RAW en
    /// couverture.
    Pairs,
    /// Les rafales : photos d'un même dossier prises à au plus
    /// `gap_seconds` secondes l'une de l'autre, la première en couverture.
    Bursts {
        /// Écart maximal entre deux photos qui se suivent, en secondes.
        gap_seconds: u32,
    },
}

/// Écarts proposés entre deux photos d'une rafale, en secondes (2 par défaut).
pub const BURST_GAP_CHOICES: [u32; 5] = [1, 2, 3, 5, 10];

/// Piles à créer, et piles d'une seule photo qu'elles remplacent.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct AutoPlan {
    before: Vec<Stack>,
    after: Vec<Stack>,
}

impl AutoPlan {
    /// Nombre de piles créées.
    #[must_use]
    pub fn stack_count(&self) -> usize {
        self.after.len()
    }

    /// Nombre de photos empilées.
    #[must_use]
    pub fn photo_count(&self) -> usize {
        self.after.iter().map(|stack| stack.members.len()).sum()
    }
}

/// Ce que l'empilement ferait, pour le résumé à confirmer.
///
/// # Errors
///
/// Échoue si la base est illisible.
pub fn plan(
    connection: &Connection,
    scope: &AutoScope,
    method: AutoMethod,
) -> Result<AutoPlan, CatalogError> {
    let candidates = match (scope, method) {
        (AutoScope::Folder(folder), _) => {
            stacks::auto_candidates(connection, CandidateScope::FolderTree(*folder))?
        }
        (AutoScope::Media(ids), AutoMethod::Pairs) => partners_of(connection, ids)?,
        (AutoScope::Media(ids), AutoMethod::Bursts { .. }) => {
            stacks::auto_candidates(connection, CandidateScope::Media(ids))?
        }
    };
    match method {
        AutoMethod::Pairs => build(connection, &candidates, pair_groups(&candidates)),
        AutoMethod::Bursts { gap_seconds } => bursts(connection, &candidates, gap_seconds),
    }
}

/// Empile, de façon annulable.
///
/// # Errors
///
/// [`StackError::NothingToStack`] s'il n'y a rien à empiler ; rien n'est
/// changé en cas d'échec.
pub fn apply(
    catalog: &mut Catalog,
    history: &mut History,
    scope: &AutoScope,
    method: AutoMethod,
) -> Result<EditSummary, StackError> {
    let plan = plan(catalog.connection(), scope, method)?;
    if plan.after.is_empty() {
        return Err(StackError::NothingToStack);
    }
    let change = match method {
        AutoMethod::Pairs => StackChange::PairsStacked,
        AutoMethod::Bursts { .. } => StackChange::BurstsStacked,
    };
    perform(catalog, history, change, plan.before, plan.after)
}

/// Empile les paires RAW + JPEG ajoutées par un import (dans la
/// transaction qui le termine, hors de Cmd+Z) ; renvoie le nombre de
/// piles créées.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn stack_imported_pairs(
    connection: &Connection,
    import: ImportId,
) -> Result<usize, CatalogError> {
    let candidates = stacks::auto_candidates(connection, CandidateScope::Import(import))?;
    let plan = build(connection, &candidates, pair_groups(&candidates))?;
    for stack in &plan.before {
        stacks::delete(connection, stack.id)?;
    }
    for stack in &plan.after {
        stacks::insert(connection, stack)?;
    }
    Ok(plan.after.len())
}

/// Empile les rafales ajoutées par un import (après ses paires, qui comptent
/// chacune comme une photo), dans la transaction qui le termine ; renvoie
/// le nombre de piles créées.
///
/// # Errors
///
/// Échoue si la base est illisible ou verrouillée en écriture.
pub fn stack_imported_bursts(
    connection: &Connection,
    import: ImportId,
    gap_seconds: u32,
) -> Result<usize, CatalogError> {
    let candidates = stacks::auto_candidates(connection, CandidateScope::Import(import))?;
    let plan = bursts(connection, &candidates, gap_seconds)?;
    for stack in &plan.before {
        stacks::delete(connection, stack.id)?;
    }
    for stack in &plan.after {
        stacks::insert(connection, stack)?;
    }
    Ok(plan.after.len())
}

/// Une photo seule, ou une pile existante, dans une rafale.
struct BurstUnit {
    /// Photos, dans l'ordre (la pile existante telle quelle).
    members: Vec<MediaId>,
    /// Pile existante qu'elle remplace (une pile d'une photo comprise).
    stack: Option<Stack>,
    /// Instant de prise de vue de sa première photo, en secondes.
    second: Option<i64>,
}

/// Instant d'une date locale `2024-07-14T18:30:05`, en secondes (les dates
/// locales se comparent entre elles, sans fuseau).
fn second_of(capture_time: &str) -> Option<i64> {
    let local: jiff::civil::DateTime = capture_time.parse().ok()?;
    let zoned = local.to_zoned(jiff::tz::TimeZone::UTC).ok()?;
    Some(zoned.timestamp().as_second())
}

/// Les rafales des photos `candidates` (déjà triées par dossier et date) :
/// dans chaque dossier, les unités qui se suivent à au plus `gap_seconds`
/// secondes forment une pile, si elles sont au moins deux.
fn bursts(
    connection: &Connection,
    candidates: &[AutoCandidate],
    gap_seconds: u32,
) -> Result<AutoPlan, CatalogError> {
    // Unités, par dossier : une pile existante compte une fois.
    let mut by_folder: BTreeMap<FolderId, Vec<BurstUnit>> = BTreeMap::new();
    let mut seen = HashSet::new();
    for candidate in candidates {
        let unit = match candidate.stack {
            Some((id, size)) if size >= 2 => {
                if !seen.insert(id) {
                    continue;
                }
                let Some(stack) = stacks::get(connection, id)? else {
                    continue;
                };
                BurstUnit {
                    members: stack.members.clone(),
                    stack: Some(stack),
                    second: second_of(&candidate.capture_time),
                }
            }
            lone => BurstUnit {
                members: vec![candidate.id],
                stack: match lone {
                    Some((id, _)) if seen.insert(id) => stacks::get(connection, id)?,
                    _ => None,
                },
                second: second_of(&candidate.capture_time),
            },
        };
        by_folder.entry(candidate.folder_id).or_default().push(unit);
    }
    let mut runs: Vec<Vec<BurstUnit>> = Vec::new();
    for units in by_folder.into_values() {
        let mut run: Vec<BurstUnit> = Vec::new();
        for unit in units {
            let follows = match (run.last().and_then(|last| last.second), unit.second) {
                (Some(previous), Some(current)) => current - previous <= i64::from(gap_seconds),
                _ => false,
            };
            if !follows && !run.is_empty() {
                runs.push(std::mem::take(&mut run));
            }
            run.push(unit);
        }
        runs.push(run);
    }
    let mut before = Vec::new();
    let mut groups = Vec::new();
    for run in runs.into_iter().filter(|run| run.len() >= 2) {
        let mut members = Vec::new();
        for unit in run {
            members.extend(unit.members);
            before.extend(unit.stack);
        }
        groups.push(members);
    }
    let first = stacks::next_id(connection)?.get();
    let after = (first..)
        .zip(groups)
        .map(|(id, members)| Stack {
            id: crate::domain::StackId::new(id),
            collapsed: true,
            members,
        })
        .collect();
    Ok(AutoPlan { before, after })
}

/// Clé d'une paire : le dossier et le nom sans extension, sans tenir
/// compte des majuscules.
fn pair_key(candidate: &AutoCandidate) -> (FolderId, String) {
    let stem = Path::new(&candidate.file_name)
        .file_stem()
        .map_or_else(String::new, |stem| stem.to_string_lossy().to_lowercase());
    (candidate.folder_id, stem)
}

/// Les photos `ids` et celles qui portent leur nom dans leur dossier.
fn partners_of(
    connection: &Connection,
    ids: &[MediaId],
) -> Result<Vec<AutoCandidate>, CatalogError> {
    let selected = stacks::auto_candidates(connection, CandidateScope::Media(ids))?;
    let keys: HashSet<_> = selected.iter().map(pair_key).collect();
    let folders: Vec<FolderId> = selected
        .iter()
        .map(|candidate| candidate.folder_id)
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    Ok(
        stacks::auto_candidates(connection, CandidateScope::Folders(&folders))?
            .into_iter()
            .filter(|candidate| keys.contains(&pair_key(candidate)))
            .collect(),
    )
}

/// Vrai pour une photo hors de toute pile d'au moins deux photos.
fn is_free(candidate: &AutoCandidate) -> bool {
    candidate.stack.is_none_or(|(_, size)| size < 2)
}

/// Les paires : au moins un RAW et un JPEG (ou HEIC) de même nom dans le
/// même dossier ; les RAW d'abord.
fn pair_groups(candidates: &[AutoCandidate]) -> Vec<Vec<MediaId>> {
    let mut groups: BTreeMap<_, Vec<&AutoCandidate>> = BTreeMap::new();
    for candidate in candidates.iter().filter(|candidate| is_free(candidate)) {
        groups
            .entry(pair_key(candidate))
            .or_default()
            .push(candidate);
    }
    groups
        .into_values()
        .filter_map(|mut group| {
            group.sort_by(|a, b| a.file_name.cmp(&b.file_name));
            let (raws, others): (Vec<_>, Vec<_>) = group
                .into_iter()
                .partition(|candidate| candidate.format.is_raw());
            let companions: Vec<_> = others
                .into_iter()
                .filter(|candidate| {
                    matches!(candidate.format, MediaFormat::Jpeg | MediaFormat::Heic)
                })
                .collect();
            (!raws.is_empty() && !companions.is_empty()).then(|| {
                raws.iter()
                    .chain(&companions)
                    .map(|candidate| candidate.id)
                    .collect()
            })
        })
        .collect()
}

/// Les piles à créer (repliées), et les piles d'une photo à défaire avant.
fn build(
    connection: &Connection,
    candidates: &[AutoCandidate],
    groups: Vec<Vec<MediaId>>,
) -> Result<AutoPlan, CatalogError> {
    let grouped: HashSet<MediaId> = groups.iter().flatten().copied().collect();
    let mut lone = Vec::new();
    let mut seen = HashSet::new();
    for candidate in candidates
        .iter()
        .filter(|candidate| grouped.contains(&candidate.id))
    {
        if let Some((stack, _)) = candidate.stack
            && seen.insert(stack)
            && let Some(stack) = stacks::get(connection, stack)?
        {
            lone.push(stack);
        }
    }
    let first = stacks::next_id(connection)?.get();
    let after = (first..)
        .zip(groups)
        .map(|(id, members)| Stack {
            id: crate::domain::StackId::new(id),
            collapsed: true,
            members,
        })
        .collect();
    Ok(AutoPlan {
        before: lone,
        after,
    })
}
