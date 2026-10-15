//! Script de compilation : prépare ce dont Tauri a besoin (configuration,
//! permissions, icônes) avant la compilation de l'application.

fn main() {
    tauri_build::build();
}
