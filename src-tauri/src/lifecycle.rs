//! Fin de vie de l'application : fermeture propre et redémarrage.
//!
//! Tauri termine le processus sans détruire l'état de l'application : le
//! catalogue et le journal doivent donc être refermés explicitement, à la
//! fermeture comme avant un redémarrage.

use std::sync::{Mutex, PoisonError};

use tauri::{AppHandle, Manager, Runtime};

use crate::catalog::Catalog;
use crate::catalog::auto_backup::AutoBackupOutcome;
use crate::commands::preferences::AppPreferences;
use crate::logging::LogGuard;
use crate::workspace::Workspace;

/// Referme proprement le catalogue (réintégration du journal WAL), le
/// sauvegarde si c'est l'heure, puis vide le journal de l'application. Peut
/// être appelé plusieurs fois sans risque (la sauvegarde n'est faite qu'une
/// fois par jour, par défaut).
pub fn release_resources<R: Runtime>(app: &AppHandle<R>) {
    if let Some(catalog) = app.try_state::<Mutex<Catalog>>() {
        close_catalog(&catalog);
        back_up_catalog(app, &catalog);
    }
    if let Some(log_guard) = app.try_state::<LogGuard>() {
        log_guard.flush();
    }
}

/// Redémarre l'application, par exemple pour ouvrir un autre catalogue.
pub fn restart<R: Runtime>(app: &AppHandle<R>) -> ! {
    tracing::info!("application restarting");
    release_resources(app);
    app.restart()
}

/// Sauvegarde automatique à la fermeture. Un échec est journalisé
/// mais n'empêche pas de fermer : le catalogue lui-même n'est pas touché.
fn back_up_catalog<R: Runtime>(app: &AppHandle<R>, catalog: &Mutex<Catalog>) {
    let (Some(workspace), Some(preferences)) = (
        app.try_state::<Workspace>(),
        app.try_state::<AppPreferences>(),
    ) else {
        return;
    };
    let policy = preferences.current().auto_backup_policy();
    let today = jiff::Zoned::now().date();
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    match catalog.back_up_if_due(&workspace.backups_dir(), policy, today) {
        Ok(AutoBackupOutcome::Created { path, removed }) => {
            tracing::info!(path = %path.display(), removed, "automatic catalog backup done");
        }
        Ok(AutoBackupOutcome::NotDue) => {
            tracing::debug!("automatic catalog backup not due yet");
        }
        Err(error) => tracing::error!(%error, "automatic catalog backup failed"),
    }
}

fn close_catalog(catalog: &Mutex<Catalog>) {
    // Un verrou « empoisonné » (panique d'un autre fil pendant qu'il le
    // tenait) protège quand même une connexion utilisable.
    let catalog = catalog.lock().unwrap_or_else(PoisonError::into_inner);
    match catalog.checkpoint() {
        Ok(()) => tracing::info!("catalog closed"),
        Err(error) => {
            tracing::error!(%error, "cannot checkpoint the catalog; changes stay in its WAL file");
        }
    }
}
