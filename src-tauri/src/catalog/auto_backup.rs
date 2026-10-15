//! Sauvegarde automatique du catalogue à la fermeture : à la
//! fréquence choisie, en ne gardant que les dernières.
//!
//! Seules les sauvegardes automatiques (`Catalogue-auto-…`) sont
//! concernées ; celles faites avant une migration ne sont jamais retirées.
//! Les sauvegardes en trop sont effacées définitivement : ce sont des copies
//! fabriquées par PicLib, pas des fichiers de l'utilisateur.

use std::path::{Path, PathBuf};

use jiff::civil::Date;
use serde::{Deserialize, Serialize};

use super::backup::{AUTOMATIC_BACKUP_PREFIX, create_automatic_backup};
use super::{BackupError, Catalog};

/// Fréquence des sauvegardes automatiques.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BackupFrequency {
    /// À chaque fermeture.
    EveryClose,
    /// À la première fermeture de chaque journée.
    Daily,
    /// À la première fermeture après sept jours.
    Weekly,
}

/// Réglages de la sauvegarde automatique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AutoBackupPolicy {
    /// Fréquence.
    pub frequency: BackupFrequency,
    /// Nombre de sauvegardes automatiques gardées (au moins 1).
    pub keep: usize,
}

/// Résultat d'une sauvegarde automatique.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AutoBackupOutcome {
    /// Sauvegarde faite ; `removed` anciennes sauvegardes effacées.
    Created {
        /// Sauvegarde créée.
        path: PathBuf,
        /// Nombre d'anciennes sauvegardes effacées.
        removed: usize,
    },
    /// Pas encore l'heure (une sauvegarde récente existe déjà).
    NotDue,
}

/// Sauvegarde automatique présente dans le dossier des sauvegardes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomaticBackup {
    /// Fichier.
    pub path: PathBuf,
    /// Jour de la sauvegarde (heure locale).
    pub date: Date,
    /// Horodatage complet et suffixe éventuel, pour les classer.
    order: (String, u32),
}

impl Catalog {
    /// Sauvegarde le catalogue si c'est l'heure (`today` : date locale du
    /// jour), puis efface les sauvegardes automatiques en trop.
    ///
    /// # Errors
    ///
    /// Échoue si la sauvegarde ne peut pas être faite ou vérifiée ; rien
    /// n'est alors effacé.
    pub fn back_up_if_due(
        &self,
        backups_dir: &Path,
        policy: AutoBackupPolicy,
        today: Date,
    ) -> Result<AutoBackupOutcome, BackupError> {
        let existing = automatic_backups(backups_dir);
        let latest = existing.last().map(|backup| backup.date);
        if !is_due(latest, today, policy.frequency) {
            return Ok(AutoBackupOutcome::NotDue);
        }
        let path = create_automatic_backup(self.connection(), backups_dir, self.schema_version())?;
        let removed = prune(backups_dir, policy.keep.max(1));
        Ok(AutoBackupOutcome::Created { path, removed })
    }
}

/// Indique si une sauvegarde est due, d'après le jour de la dernière.
#[must_use]
pub fn is_due(latest: Option<Date>, today: Date, frequency: BackupFrequency) -> bool {
    let Some(latest) = latest else {
        return true;
    };
    match frequency {
        BackupFrequency::EveryClose => true,
        BackupFrequency::Daily => latest < today,
        BackupFrequency::Weekly => today
            .since(latest)
            .map_or(true, |span| span.get_days() >= 7),
    }
}

/// Sauvegardes automatiques du dossier, de la plus ancienne à la plus
/// récente. Les autres fichiers sont ignorés.
#[must_use]
pub fn automatic_backups(backups_dir: &Path) -> Vec<AutomaticBackup> {
    let Ok(entries) = std::fs::read_dir(backups_dir) else {
        return Vec::new();
    };
    let mut backups: Vec<AutomaticBackup> = entries
        .filter_map(Result::ok)
        .filter_map(|entry| parse(&entry.path()))
        .collect();
    backups.sort_by(|a, b| a.order.cmp(&b.order));
    backups
}

/// Efface les sauvegardes automatiques les plus anciennes pour n'en garder
/// que `keep` ; renvoie le nombre de fichiers effacés.
fn prune(backups_dir: &Path, keep: usize) -> usize {
    let backups = automatic_backups(backups_dir);
    let excess = backups.len().saturating_sub(keep);
    let mut removed = 0;
    for backup in backups.into_iter().take(excess) {
        match std::fs::remove_file(&backup.path) {
            Ok(()) => {
                removed += 1;
                tracing::info!(path = %backup.path.display(), "old automatic backup removed");
            }
            Err(error) => {
                tracing::warn!(%error, path = %backup.path.display(), "cannot remove an old backup");
            }
        }
    }
    removed
}

/// Lit le nom `Catalogue-auto-AAAAMMJJ-HHMMSS[-N].sqlite`.
fn parse(path: &Path) -> Option<AutomaticBackup> {
    if path.extension()? != "sqlite" {
        return None;
    }
    let stem = path.file_stem()?.to_str()?;
    let rest = stem
        .strip_prefix(AUTOMATIC_BACKUP_PREFIX)?
        .strip_prefix('-')?;
    let (stamp, suffix) = match rest.get(15..) {
        Some("") => (rest, 1),
        Some(suffix) => (rest.get(..15)?, suffix.strip_prefix('-')?.parse().ok()?),
        None => return None,
    };
    let date = Date::strptime("%Y%m%d", stamp.get(..8)?).ok()?;
    Some(AutomaticBackup {
        path: path.to_path_buf(),
        date,
        order: (stamp.to_owned(), suffix),
    })
}
