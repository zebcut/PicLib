//! Reprise, au démarrage, des opérations interrompues par un arrêt brutal
//!, en tâche de fond : vérifier une copie peut prendre
//! quelques secondes.

use std::sync::Mutex;

use jiff::tz::TimeZone;
use tauri::{AppHandle, Manager};

use crate::catalog::Catalog;
use crate::interrupted::resume_interrupted;

/// Lance la reprise sans bloquer le démarrage. Si des médias ont été
/// inscrits ou retirés, les tâches de fond (vignettes, lieux) sont relancées ;
/// puis les fichiers manquants sont recherchés.
pub fn resume_in_background(app: &AppHandle) {
    let app = app.clone();
    let spawned = std::thread::Builder::new()
        .name("resume-interrupted".to_owned())
        .spawn(move || {
            let Some(catalog) = app.try_state::<Mutex<Catalog>>() else {
                return;
            };
            match resume_interrupted(&catalog, &TimeZone::system()) {
                Ok(report) if report.changed_media() => {
                    super::thumbnails::request_thumbnails(&app);
                    super::places::request_place_lookup(&app);
                }
                Ok(_) => {}
                Err(error) => {
                    tracing::error!(%error, "cannot resume the interrupted operations");
                }
            }
            // Une fois le journal terminé, les fichiers sont à leur place
            // définitive : on cherche ceux qui manquent.
            super::missing::request_missing_check(&app);
        });
    if let Err(error) = spawned {
        tracing::error!(%error, "cannot start resuming the interrupted operations");
    }
}
