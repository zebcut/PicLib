//! Génération de `src/services/bindings.ts` à partir du registre des
//! commandes.

use std::path::PathBuf;

use specta_typescript::Typescript;

use super::specta_builder;

/// Chemin du fichier TypeScript généré, dans le dossier du frontend.
///
/// `CARGO_MANIFEST_DIR` est fourni par Cargo à la compilation : aucun chemin
/// n'est écrit en dur dans le code.
fn bindings_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("src")
        .join("services")
        .join("bindings.ts")
}

/// Exporteur TypeScript, configuré à un seul endroit pour que l'application
/// et les tests produisent exactement le même fichier.
fn typescript_exporter() -> Typescript {
    Typescript::default()
}

/// Régénère `src/services/bindings.ts` au lancement en développement, pour
/// que l'interface voie immédiatement une commande ajoutée ou modifiée.
///
/// Un échec n'empêche pas l'application de démarrer : il est journalisé, et
/// le test `bindings_file_is_up_to_date` le détectera de toute façon.
#[cfg(debug_assertions)]
pub fn export_for_development() {
    let path = bindings_path();
    match specta_builder().export(typescript_exporter(), &path) {
        Ok(()) => tracing::debug!(path = %path.display(), "TypeScript bindings exported"),
        Err(error) => tracing::error!(%error, "failed to export TypeScript bindings"),
    }
}
