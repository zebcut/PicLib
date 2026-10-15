//! Tâches de fond : travaux longs (analyse, import, vignettes...) exécutés
//! hors du fil de l'interface, avec progression et annulation.
//!
//! Chaque tâche tourne sur son propre fil d'exécution. Ses changements d'état
//! (démarrage, progression, fin) sont diffusés aux abonnés, par exemple
//! l'interface qui affiche l'indicateur de tâches.

mod cancellation;
mod manager;
mod pass_scheduler;

pub use cancellation::CancellationToken;
pub use manager::{JobContext, JobError, JobId, JobManager, JobSnapshot, JobStatus, StartError};
pub use pass_scheduler::{EndGuard, PassScheduler};
