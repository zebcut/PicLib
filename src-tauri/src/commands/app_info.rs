//! Informations générales sur l'application (nom, version).

use serde::Serialize;
use specta::Type;
use tauri::{AppHandle, PackageInfo};

/// Nom et version de l'application, tels que déclarés dans
/// `tauri.conf.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Type)]
pub struct AppInfo {
    /// Nom affiché de l'application.
    pub name: String,
    /// Version au format « semver » (ex. `0.1.0`).
    pub version: String,
}

impl From<&PackageInfo> for AppInfo {
    fn from(package: &PackageInfo) -> Self {
        Self {
            name: package.name.clone(),
            version: package.version.to_string(),
        }
    }
}

/// Renvoie le nom et la version de l'application.
// Tauri exige que les paramètres injectés (`AppHandle`) soient passés par
// valeur : l'avertissement de Clippy ne s'applique pas ici.
#[allow(clippy::needless_pass_by_value)]
#[tauri::command]
#[specta::specta]
pub fn get_app_info(app: AppHandle) -> AppInfo {
    tracing::debug!("application info requested");
    AppInfo::from(app.package_info())
}
