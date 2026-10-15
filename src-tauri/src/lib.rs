//! Cœur de PicLib : démarrage de l'application Tauri, catalogue, journalisation
//! et commandes exposées à l'interface.
//!
//! L'organisation en couches (domaine, services, commandes) est décrite dans
//! `ARCHITECTURE.md`.

pub mod catalog;
pub mod catalog_selection;
pub mod collections;
mod commands;
pub mod deletion;
pub mod domain;
pub mod edits;
pub mod external_editing;
pub mod files;
pub mod filter_presets;
pub mod folders;
pub mod import;
pub mod interrupted;
pub mod jobs;
pub mod keywords;
mod lifecycle;
mod logging;
pub mod map_tiles;
pub mod media_files;
pub mod metadata;
pub mod metadata_templates;
pub mod missing_files;
pub mod moving;
pub mod places;
pub mod platform;
pub mod protocol;
pub mod relocation;
pub mod renaming;
pub mod replaced_files;
pub mod rereading;
pub mod settings;
pub mod stacks;
pub mod startup;
pub mod synchronization;
pub mod thumbnails;
pub mod workspace;
pub mod xmp_files;

use std::sync::Mutex;

use tauri::{App, Manager, RunEvent};

use thumbnails::{ImportPreviewCache, PreviewCache};

use commands::catalog_selection::PendingCatalogCreation;
use commands::file_metadata::PendingFileMetadata;
use commands::folder_analysis::AnalyzedFolders;
use commands::import_destination::ChosenDestinations;
use commands::jobs::AppJobManager;
use commands::keyword_transfer::PendingKeywordImport;
use commands::places::{InterfaceLanguage, PlaceScheduler};
use commands::preferences::AppPreferences;
use commands::startup::StartupStatus;
use commands::thumbnails::ThumbnailScheduler;

/// Construit et lance l'application, puis bloque jusqu'à sa fermeture.
///
/// # Errors
///
/// Renvoie une erreur si Tauri ne parvient pas à créer l'application
/// (fenêtre, configuration invalide...).
pub fn run() -> tauri::Result<()> {
    let specta_builder = commands::specta_builder();

    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .register_asynchronous_uri_scheme_protocol(protocol::SCHEME, protocol::handle)
        .invoke_handler(specta_builder.invoke_handler())
        .setup(|app| {
            let log_guard = logging::init_for_app(app.handle());
            app.manage(log_guard);

            #[cfg(debug_assertions)]
            commands::bindings::export_for_development();

            let package = app.package_info();
            tracing::info!(name = %package.name, version = %package.version, "application started");

            // Les tâches de fond doivent être prêtes avant l'ouverture du
            // catalogue, qui lance la fabrication des vignettes manquantes.
            app.manage(AppJobManager::new());
            app.manage(ThumbnailScheduler::default());
            app.manage(PlaceScheduler::default());
            app.manage(commands::missing::MissingScheduler::default());
            app.manage(commands::xmp::XmpAutoScheduler::default());
            app.manage(InterfaceLanguage::default());
            app.manage(PendingCatalogCreation::default());
            app.manage(AnalyzedFolders::default());
            app.manage(ChosenDestinations::default());
            app.manage(commands::import_sources::ImportSources::default());
            app.manage(commands::import_requests::ImportRequests::default());
            app.manage(commands::import_requests::ImportedCards::default());
            app.manage(PendingKeywordImport::default());
            app.manage(PendingFileMetadata::default());
            // Historique des modifications de la session (Cmd+Z).
            app.manage(Mutex::new(edits::History::default()));
            let preferences = AppPreferences::load_for_app(app.handle());
            let preview_cache_limit = preferences.current().preview_cache_limit_bytes();
            app.manage(preferences);
            app.manage(commands::external_editors::AppEditors::load_for_app(
                app.handle(),
            ));
            open_catalog(app, preview_cache_limit);
            // Après un arrêt brutal : termine ce que le journal laisse en
            // attente (copies à inscrire, fichiers partis à la corbeille).
            commands::interrupted::resume_in_background(app.handle());
            commands::thumbnails::request_thumbnails(app.handle());
            // Fonds de carte de la vue Carte, gardés dans le cache de
            // l'application (pas dans le catalogue : ils ne lui sont pas
            // propres).
            if let Ok(cache_dir) = app.path().app_cache_dir() {
                let package = app.package_info();
                let user_agent = format!(
                    "PicLib/{} (+https://github.com/zebcut/PicLib)",
                    package.version
                );
                // Ancien nom gardé s'il existe (cache déjà rempli).
                let former = cache_dir.join("Fonds de carte");
                let tiles_dir = if former.is_dir() {
                    former
                } else {
                    cache_dir.join("Map tiles")
                };
                app.manage(map_tiles::MapTileCache::new(
                    tiles_dir,
                    map_tiles::CACHE_LIMIT_BYTES,
                    Box::new(map_tiles::OsmTiles::new(&user_agent)),
                ));
            }
            // Une carte branchée peut ouvrir l'écran d'import (préférence).
            commands::import_requests::watch_cards(app.handle().clone());
            // Les XMP s'écrivent seuls après les modifications (préférence).
            commands::xmp::watch_pending_xmp(app.handle().clone());
            // Les exports des éditeurs externes rejoignent le catalogue
            //.
            commands::editor_watch::watch_edited_files(app.handle().clone());
            Ok(())
        })
        // Un dossier ou des fichiers glissés du Finder ouvrent l'écran
        // d'import.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) = event {
                commands::import_requests::dropped(window.app_handle(), paths);
            }
        })
        .build(tauri::generate_context!())?;

    app.run(|app_handle, event| {
        // Photos remises à PicLib par un éditeur (« Exporter vers une
        // application ») ou par « Ouvrir avec » du Finder.
        #[cfg(target_os = "macos")]
        if let RunEvent::Opened { urls } = &event {
            let paths: Vec<std::path::PathBuf> = urls
                .iter()
                .filter_map(|url| url.to_file_path().ok())
                .collect();
            if !paths.is_empty() {
                // Hors du fil de l'interface : le catalogue peut être occupé.
                let handle = app_handle.clone();
                std::thread::spawn(move || commands::editor_watch::opened(&handle, &paths));
            }
        }
        if let RunEvent::Exit = event {
            tracing::info!("application exiting");
            lifecycle::release_resources(app_handle);
        }
    });
    Ok(())
}

/// Ouvre le catalogue et enregistre l'état du démarrage pour l'interface,
/// ainsi que le cache des aperçus du dossier de travail.
///
/// Un échec n'arrête pas l'application : l'interface affiche alors un écran
/// d'erreur expliquant le problème et permettant de choisir un autre
/// catalogue.
fn open_catalog(app: &App, preview_cache_limit: u64) {
    let status = match startup::open_for_app(app.handle()) {
        Ok(opened) => {
            let status = StartupStatus::ready(&opened);
            app.manage(PreviewCache::new(
                opened.workspace.previews_dir(),
                preview_cache_limit,
            ));
            // Vignettes laissées par un écran d'import avant un arrêt brutal.
            let import_previews = ImportPreviewCache::new(opened.workspace.import_previews_dir());
            import_previews.clear();
            app.manage(import_previews);
            app.manage(opened.workspace);
            // Une seule connexion, protégée par un verrou : les commandes y
            // accèdent chacune à son tour.
            app.manage(Mutex::new(opened.catalog));
            status
        }
        Err(error) => {
            tracing::error!(%error, "cannot open the catalog");
            StartupStatus::failed(&error)
        }
    };
    app.manage(status);
}
