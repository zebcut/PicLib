//! Exécutable de PicLib : délègue tout à la bibliothèque `piclib_lib`.

// Évite l'ouverture d'une console en plus de la fenêtre sous Windows, en
// version finale. Ne pas retirer.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::process::ExitCode;

fn main() -> ExitCode {
    match piclib_lib::run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            // Le journal peut ne pas être initialisé si l'échec survient très
            // tôt : on écrit donc aussi sur la sortie d'erreur.
            tracing::error!(%error, "application stopped with an error");
            eprintln!("PicLib: fatal error: {error}");
            ExitCode::FAILURE
        }
    }
}
