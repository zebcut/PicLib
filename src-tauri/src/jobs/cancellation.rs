//! Demande d'annulation partagée entre l'interface et une tâche.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

/// Jeton d'annulation : l'interface le déclenche, la tâche le consulte
/// régulièrement et s'arrête proprement dès qu'il est levé.
///
/// Les copies d'un jeton (`clone`) partagent le même état.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken(Arc<AtomicBool>);

impl CancellationToken {
    /// Nouveau jeton, non déclenché.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Demande l'annulation. La tâche s'arrêtera à sa prochaine vérification.
    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    /// Indique si l'annulation a été demandée.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
