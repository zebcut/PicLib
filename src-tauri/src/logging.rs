//! Journalisation structurée de l'application.
//!
//! Les messages sont écrits dans un fichier par jour dans le dossier de logs
//! du système (sur macOS : `~/Library/Logs/<identifiant de l'app>/`). En
//! développement, ils sont aussi affichés dans le terminal.
//!
//! Le niveau de détail se règle avec la variable d'environnement
//! `PICLIB_LOG` (syntaxe `EnvFilter` de `tracing-subscriber`, par exemple
//! `PICLIB_LOG=debug`).

use std::path::Path;
use std::sync::Mutex;

use tauri::{AppHandle, Manager, Runtime};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{InitError, RollingFileAppender, Rotation};
use tracing_subscriber::EnvFilter;
use tracing_subscriber::layer::SubscriberExt;
use tracing_subscriber::util::{SubscriberInitExt, TryInitError};

/// Nom de la variable d'environnement qui règle le niveau de journalisation.
const LOG_FILTER_ENV: &str = "PICLIB_LOG";

/// Préfixe des fichiers de log (`piclib.2026-09-23.log`).
const LOG_FILE_PREFIX: &str = "piclib";

/// Nombre de fichiers journaliers conservés : au-delà, les plus anciens sont
/// supprimés par `tracing-appender`. Deux semaines suffisent pour analyser un
/// problème signalé sans laisser grossir le dossier indéfiniment.
const MAX_LOG_FILES: usize = 14;

/// Erreurs possibles lors de la mise en place de la journalisation.
#[derive(Debug, thiserror::Error)]
pub enum LoggingError {
    /// Le système n'a pas fourni de dossier de logs pour l'application.
    #[error("unable to resolve the log directory: {0}")]
    LogDirectory(#[source] tauri::Error),
    /// Le dossier de logs n'a pas pu être créé.
    #[error("unable to create the log directory: {0}")]
    CreateDirectory(#[source] std::io::Error),
    /// Le fichier de log n'a pas pu être créé.
    #[error("unable to create the log file: {0}")]
    FileAppender(#[from] InitError),
    /// Un autre système de journalisation est déjà installé.
    #[error("unable to install the logger: {0}")]
    Subscriber(#[from] TryInitError),
}

/// Garde qui doit rester en vie tant que l'application tourne.
///
/// L'écriture dans le fichier se fait sur un fil séparé pour ne jamais
/// ralentir l'application ; quand la garde interne est détruite, les derniers
/// messages en attente sont écrits sur le disque.
///
/// Sur macOS, Tauri termine le processus sans détruire son état : il faut
/// donc appeler [`LogGuard::flush`] explicitement à la fermeture.
pub struct LogGuard {
    // `Mutex` permet de retirer la garde (et donc de vider le journal) depuis
    // l'état partagé de Tauri, qui n'est accessible qu'en lecture.
    worker: Mutex<Option<WorkerGuard>>,
}

impl LogGuard {
    fn new(worker: Option<WorkerGuard>) -> Self {
        Self {
            worker: Mutex::new(worker),
        }
    }

    /// Écrit sur le disque les messages en attente et arrête l'écriture en
    /// arrière-plan. Les messages suivants ne sont plus écrits dans le
    /// fichier : à n'appeler qu'à la fermeture de l'application.
    pub fn flush(&self) {
        // Un verrou « empoisonné » (panique d'un autre fil pendant qu'il le
        // tenait) contient quand même une garde valide : on la vide aussi.
        let worker = match self.worker.lock() {
            Ok(mut worker) => worker.take(),
            Err(poisoned) => poisoned.into_inner().take(),
        };
        drop(worker);
    }
}

/// Installe la journalisation pour l'application Tauri.
///
/// Si le fichier de log ne peut pas être créé, l'application démarre quand
/// même, avec une journalisation limitée à la sortie d'erreur : ne plus avoir
/// de fichier de log est gênant, mais ne justifie pas de refuser de démarrer.
pub fn init_for_app<R: Runtime>(app: &AppHandle<R>) -> LogGuard {
    let result = app
        .path()
        .app_log_dir()
        .map_err(LoggingError::LogDirectory)
        .and_then(|log_dir| init_with_file(&log_dir));

    match result {
        Ok(worker) => LogGuard::new(Some(worker)),
        Err(error) => {
            // Si l'échec vient de l'installation elle-même, cette seconde
            // tentative échouera aussi ; le message sur la sortie d'erreur
            // garantit qu'on en garde au moins une trace.
            let fallback_installed = init_stderr_only().is_ok();
            eprintln!("PicLib: file logging disabled: {error}");
            if fallback_installed {
                tracing::error!(%error, "file logging disabled, logging to stderr only");
            }
            LogGuard::new(None)
        }
    }
}

/// Installe la journalisation vers des fichiers journaliers dans `log_dir`
/// (créé si besoin), et vers le terminal en développement.
///
/// # Errors
///
/// Renvoie une erreur si le dossier ou le fichier de log ne peut pas être
/// créé, ou si une journalisation est déjà installée dans le processus.
pub fn init_with_file(log_dir: &Path) -> Result<WorkerGuard, LoggingError> {
    // `tracing-appender` fait le ménage des anciens fichiers avant de créer
    // le dossier, et signale une erreur sur le terminal au premier lancement :
    // on crée donc le dossier nous-mêmes. Il s'agit d'un dossier interne de
    // l'application, pas d'un fichier de l'utilisateur.
    std::fs::create_dir_all(log_dir).map_err(LoggingError::CreateDirectory)?;
    let file_appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix(LOG_FILE_PREFIX)
        .filename_suffix("log")
        .max_log_files(MAX_LOG_FILES)
        .build(log_dir)?;
    let (file_writer, worker) = tracing_appender::non_blocking(file_appender);

    let file_layer = tracing_subscriber::fmt::layer()
        .with_writer(file_writer)
        .with_ansi(false);
    let stderr_layer = cfg!(debug_assertions)
        .then(|| tracing_subscriber::fmt::layer().with_writer(std::io::stderr));

    tracing_subscriber::registry()
        .with(log_filter())
        .with(file_layer)
        .with(stderr_layer)
        .try_init()?;
    Ok(worker)
}

/// Installe une journalisation de secours, vers la sortie d'erreur seulement.
fn init_stderr_only() -> Result<(), TryInitError> {
    tracing_subscriber::registry()
        .with(log_filter())
        .with(tracing_subscriber::fmt::layer().with_writer(std::io::stderr))
        .try_init()
}

/// Niveau des messages de `nom_exif` (lecture des EXIF). Il écrit au niveau
/// « info » chaque balise lue (données fabricant en hexadécimal comprises),
/// et avertit pour chaque photo sans GPS, ce qui est normal : des centaines
/// de Mo de journal sur un gros import. Seules ses erreurs sont gardées ;
/// PicLib journalise lui-même les métadonnées illisibles.
const EXIF_READER_DIRECTIVE: &str = "nom_exif=error";

/// Filtre de niveau : `PICLIB_LOG` s'il est défini et valide, sinon un niveau
/// par défaut plus bavard en développement pour le code de PicLib.
fn log_filter() -> EnvFilter {
    let default_directives = if cfg!(debug_assertions) {
        format!("info,piclib_lib=debug,piclib=debug,{EXIF_READER_DIRECTIVE}")
    } else {
        format!("info,{EXIF_READER_DIRECTIVE}")
    };
    EnvFilter::try_from_env(LOG_FILTER_ENV).unwrap_or_else(|_| EnvFilter::new(default_directives))
}
