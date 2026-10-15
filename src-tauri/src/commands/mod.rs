//! Commandes Tauri exposées à l'interface.
//!
//! Les commandes restent fines : elles valident les entrées et délèguent aux
//! services du cœur. Chaque commande est annotée avec `#[specta::specta]` et
//! enregistrée dans [`specta_builder`] : c'est ce qui permet de générer
//! automatiquement le fichier TypeScript `src/services/bindings.ts` (voir le
//! module `bindings`), qu'il ne faut jamais modifier à la main.
//!
//! Une commande qui lit ou écrit le catalogue, ou touche au disque, est
//! `async` : Tauri exécute une commande synchrone sur le fil principal, qui
//! fait vivre la fenêtre, et une lecture un peu longue y figerait
//! l'interface.

mod app_info;
pub mod capture_time;
pub mod catalog_info;
pub mod catalog_selection;
mod collection_types;
pub mod collections;
pub mod deletion;
mod dialogs;
pub mod editor_watch;
pub mod edits;
pub mod external_editors;
pub mod file_metadata;
pub mod filter_presets;
pub mod folder_analysis;
pub mod folder_deletion;
pub mod folder_move;
pub mod folders;
pub mod grid_filter;
pub mod import;
pub mod import_destination;
pub mod import_presets;
pub mod import_requests;
pub mod import_sources;
pub mod interrupted;
pub mod jobs;
pub mod keyword_list;
pub mod keyword_sets;
pub mod keyword_transfer;
pub mod keywords;
pub mod map;
pub mod media_move;
pub mod metadata;
pub mod metadata_templates;
pub mod missing;
pub mod operations_log;
pub mod places;
pub mod preferences;
pub mod relocation;
pub mod renaming;
pub mod stacks;
pub mod startup;
pub mod synchronization;
pub mod thumbnails;
pub mod xmp;
// La génération du fichier TypeScript ne sert qu'en développement et dans les
// tests : elle n'est pas incluse dans la version finale.
#[cfg(any(debug_assertions, test))]
pub mod bindings;

use tauri_specta::{Builder, collect_commands};

/// Crée le registre des commandes, partagé entre l'application (pour
/// recevoir les appels) et la génération des types TypeScript.
// Une simple liste de toutes les commandes, tenue en un seul endroit : sa
// longueur suit leur nombre.
#[allow(clippy::too_many_lines)]
pub fn specta_builder() -> Builder<tauri::Wry> {
    Builder::<tauri::Wry>::new().commands(collect_commands![
        app_info::get_app_info,
        startup::get_startup_status,
        catalog_selection::open_catalog,
        catalog_selection::create_catalog,
        catalog_selection::confirm_catalog_creation,
        jobs::subscribe_jobs,
        jobs::cancel_job,
        import_sources::list_import_sources,
        import_sources::choose_import_source,
        import_sources::analyze_import_source,
        import_sources::forget_import_source,
        import_sources::get_analyzed_files,
        import_sources::get_destination_preview,
        import_sources::forget_analysis,
        import::import_analyzed_folder,
        import_destination::get_import_preferences,
        import_destination::choose_import_destination,
        import_destination::choose_backup_destination,
        import_presets::get_import_presets,
        import_requests::subscribe_import_requests,
        import_requests::eject_card,
        import_presets::save_import_preset,
        import_presets::apply_import_preset,
        import_presets::delete_import_preset,
        catalog_info::get_catalog_overview,
        catalog_info::get_grid_count,
        catalog_info::get_grid_page,
        catalog_info::get_grid_ids,
        map::get_map_view,
        map::get_map_area_ids,
        map::get_location_preview,
        map::set_location,
        map::remove_location,
        catalog_info::get_metadata_column,
        filter_presets::get_filter_presets,
        filter_presets::create_filter_preset,
        filter_presets::update_filter_preset,
        filter_presets::rename_filter_preset,
        filter_presets::delete_filter_preset,
        filter_presets::get_source_filter,
        filter_presets::save_source_filter,
        collections::get_collections,
        collections::get_collection_deletion,
        collections::change_collections,
        collections::get_quick_collection,
        collections::set_collection_target,
        missing::check_missing_files,
        operations_log::get_operations,
        operations_log::get_operation_items,
        relocation::choose_relocated_file,
        relocation::choose_relocated_folder,
        relocation::relocate_media,
        relocation::get_folder_relocation,
        relocation::relocate_folder,
        synchronization::get_folder_sync,
        media_move::get_media_move,
        media_move::move_media,
        media_move::move_media_to_new_folder,
        media_move::rename_folder,
        renaming::get_rename_preview,
        renaming::rename_media,
        renaming::get_rename_templates,
        renaming::save_rename_template,
        renaming::delete_rename_template,
        synchronization::import_new_files,
        stacks::change_stacks,
        stacks::toggle_stacks,
        stacks::get_auto_stacking,
        catalog_info::check_media_file,
        edits::set_triage,
        edits::rotate,
        edits::undo_edit,
        edits::redo_edit,
        edits::get_edit_history,
        metadata::get_metadata_panel,
        metadata::set_metadata,
        metadata_templates::get_metadata_templates,
        metadata_templates::save_metadata_template,
        metadata_templates::delete_metadata_template,
        metadata_templates::apply_metadata_template,
        capture_time::change_capture_time,
        file_metadata::read_file_metadata,
        file_metadata::apply_file_metadata,
        file_metadata::discard_file_metadata,
        xmp::save_metadata_to_files,
        xmp::check_folder_sidecars,
        xmp::read_sidecar_metadata,
        external_editors::get_external_editors,
        external_editors::add_external_editor,
        external_editors::remove_external_editor,
        external_editors::set_default_external_editor,
        external_editors::get_external_edit_preview,
        external_editors::edit_in_external_editor,
        external_editors::reveal_in_finder,
        editor_watch::get_followed_photos,
        editor_watch::stop_following,
        editor_watch::get_edit_info,
        editor_watch::trash_unmodified_copy,
        thumbnails::refresh_replaced_files,
        keywords::get_keyword_panel,
        keywords::add_keywords,
        keywords::remove_keyword,
        keywords::suggest_keywords,
        keywords::get_suggested_keywords,
        folders::get_folder_panel,
        folder_deletion::get_folder_deletion,
        folder_deletion::remove_folder_from_catalog,
        folder_deletion::trash_folder,
        folder_move::get_folder_move,
        folder_move::move_folder,
        keyword_list::get_keyword_list,
        keyword_list::add_keyword,
        keyword_list::create_keyword,
        keyword_list::rename_keyword,
        keyword_list::move_keyword,
        keyword_list::merge_keyword,
        keyword_list::delete_keyword,
        keyword_list::delete_unused_keywords,
        keyword_list::set_keyword_export,
        keyword_sets::get_keyword_sets,
        keyword_sets::create_keyword_set,
        keyword_sets::rename_keyword_set,
        keyword_sets::delete_keyword_set,
        keyword_sets::activate_keyword_set,
        keyword_sets::fill_keyword_set,
        keyword_transfer::preview_keyword_import,
        keyword_transfer::import_keywords,
        keyword_transfer::export_keywords,
        places::set_interface_language,
        deletion::get_rejected,
        deletion::remove_rejected_from_catalog,
        deletion::trash_rejected,
        deletion::get_photo_deletion,
        deletion::remove_photos_from_catalog,
        deletion::trash_photos,
        preferences::get_preferences,
        preferences::set_preferences,
    ])
}
