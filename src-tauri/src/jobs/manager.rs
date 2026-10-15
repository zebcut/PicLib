//! Lancement, suivi et annulation des tâches de fond.

use std::cell::Cell;
use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

use super::CancellationToken;

/// Intervalle minimal entre deux annonces de progression : au-delà de dix par
/// seconde, l'interface travaillerait pour rien.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(100);

/// Identifiant d'une tâche, unique pendant la vie de l'application.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct JobId(u32);

impl JobId {
    /// Crée un identifiant à partir de sa valeur (reçue de l'interface).
    #[must_use]
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Valeur de l'identifiant.
    #[must_use]
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// État d'une tâche. `R` est le type de son résultat, `F` celui de ses échecs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobStatus<R, F> {
    /// En cours ; `progress` compte les éléments traités.
    Running {
        /// Nombre d'éléments traités (fichiers parcourus...).
        progress: u64,
        /// Nombre total d'éléments à traiter, s'il est connu.
        total: Option<u64>,
    },
    /// Terminée avec succès.
    Completed(R),
    /// Terminée sur une erreur prévue (dossier introuvable...).
    Failed(F),
    /// Arrêtée à la demande de l'utilisateur.
    Cancelled,
    /// Interrompue par une erreur de programmation (panique) : l'application
    /// continue, mais la tâche n'a pas abouti.
    Crashed,
}

/// Photographie de l'état d'une tâche, diffusée aux abonnés.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobSnapshot<K, R, F> {
    /// Identifiant de la tâche.
    pub id: JobId,
    /// Nature de la tâche et ses paramètres (dossier analysé...).
    pub kind: K,
    /// État actuel.
    pub status: JobStatus<R, F>,
}

/// Fin anormale d'une tâche, renvoyée par son travail.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JobError<F> {
    /// Le travail s'est arrêté parce que l'annulation a été demandée.
    Cancelled,
    /// Le travail a échoué sur une erreur prévue.
    Failed(F),
}

/// Erreur au lancement d'une tâche.
#[derive(Debug, thiserror::Error)]
pub enum StartError {
    /// Le système n'a pas pu créer de fil d'exécution.
    #[error("cannot start a background thread: {0}")]
    Thread(#[from] std::io::Error),
}

/// Abonné aux changements d'état ; renvoie `false` pour se désabonner (par
/// exemple quand l'interface qui écoutait a disparu).
type Subscriber<K, R, F> = Box<dyn Fn(&JobSnapshot<K, R, F>) -> bool + Send>;

/// Gestionnaire des tâches de fond. Les copies (`clone`) partagent les mêmes
/// tâches et abonnés.
pub struct JobManager<K, R, F> {
    shared: Arc<Shared<K, R, F>>,
}

impl<K, R, F> Clone for JobManager<K, R, F> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

struct Shared<K, R, F> {
    next_id: AtomicU32,
    state: Mutex<State<K, R, F>>,
}

struct State<K, R, F> {
    running: HashMap<JobId, RunningJob<K>>,
    subscribers: Vec<Subscriber<K, R, F>>,
}

struct RunningJob<K> {
    kind: K,
    progress: u64,
    total: Option<u64>,
    cancellation: CancellationToken,
}

impl<K, R, F> Default for JobManager<K, R, F>
where
    K: Clone + Send + 'static,
    R: Clone + Send + 'static,
    F: Clone + Send + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<K, R, F> JobManager<K, R, F>
where
    K: Clone + Send + 'static,
    R: Clone + Send + 'static,
    F: Clone + Send + 'static,
{
    /// Gestionnaire sans tâche ni abonné.
    #[must_use]
    pub fn new() -> Self {
        Self {
            shared: Arc::new(Shared {
                next_id: AtomicU32::new(1),
                state: Mutex::new(State {
                    running: HashMap::new(),
                    subscribers: Vec::new(),
                }),
            }),
        }
    }

    /// Ajoute un abonné. Il reçoit aussitôt l'état des tâches en cours, puis
    /// chaque changement d'état.
    pub fn subscribe(&self, subscriber: impl Fn(&JobSnapshot<K, R, F>) -> bool + Send + 'static) {
        let mut state = self.lock();
        let still_subscribed = state.running.iter().all(|(id, job)| {
            subscriber(&JobSnapshot {
                id: *id,
                kind: job.kind.clone(),
                status: JobStatus::Running {
                    progress: job.progress,
                    total: job.total,
                },
            })
        });
        if still_subscribed {
            state.subscribers.push(Box::new(subscriber));
        }
    }

    /// Lance `work` sur un nouveau fil d'exécution et renvoie l'identifiant
    /// de la tâche.
    ///
    /// `work` reçoit un [`JobContext`] pour annoncer sa progression et
    /// vérifier l'annulation. Si l'annulation a été demandée, la tâche est
    /// considérée comme annulée quel que soit ce que `work` renvoie : un
    /// travail interrompu peut ainsi renvoyer [`JobError::Cancelled`] comme
    /// une erreur.
    ///
    /// # Errors
    ///
    /// Échoue si le système refuse de créer un fil d'exécution.
    pub fn start<W>(&self, kind: K, work: W) -> Result<JobId, StartError>
    where
        W: FnOnce(&JobContext<K, R, F>) -> Result<R, JobError<F>> + Send + 'static,
    {
        let id = JobId(self.shared.next_id.fetch_add(1, Ordering::Relaxed));
        let cancellation = CancellationToken::new();
        self.register(id, kind, cancellation.clone());

        let context = JobContext {
            id,
            manager: self.clone(),
            cancellation,
            last_report: Cell::new(None),
        };
        let spawned = std::thread::Builder::new()
            .name(format!("job-{}", id.0))
            .spawn(move || context.run(work));

        if let Err(error) = spawned {
            self.lock().running.remove(&id);
            return Err(StartError::Thread(error));
        }
        Ok(id)
    }

    /// Demande l'annulation d'une tâche en cours. Renvoie `false` si elle
    /// n'est pas (ou plus) en cours.
    pub fn cancel(&self, id: JobId) -> bool {
        match self.lock().running.get(&id) {
            Some(job) => {
                job.cancellation.cancel();
                tracing::info!(job = id.0, "job cancellation requested");
                true
            }
            None => false,
        }
    }

    /// Nombre de tâches en cours.
    #[must_use]
    pub fn running_count(&self) -> usize {
        self.lock().running.len()
    }

    fn register(&self, id: JobId, kind: K, cancellation: CancellationToken) {
        let mut state = self.lock();
        state.running.insert(
            id,
            RunningJob {
                kind: kind.clone(),
                progress: 0,
                total: None,
                cancellation,
            },
        );
        publish(
            &mut state,
            &JobSnapshot {
                id,
                kind,
                status: JobStatus::Running {
                    progress: 0,
                    total: None,
                },
            },
        );
    }

    fn update_progress(&self, id: JobId, progress: u64, total: Option<u64>) {
        let mut state = self.lock();
        let Some(job) = state.running.get_mut(&id) else {
            return;
        };
        job.progress = progress;
        job.total = total.or(job.total);
        let snapshot = JobSnapshot {
            id,
            kind: job.kind.clone(),
            status: JobStatus::Running {
                progress,
                total: job.total,
            },
        };
        publish(&mut state, &snapshot);
    }

    fn finish(&self, id: JobId, status: JobStatus<R, F>) {
        let mut state = self.lock();
        let Some(job) = state.running.remove(&id) else {
            return;
        };
        publish(
            &mut state,
            &JobSnapshot {
                id,
                kind: job.kind,
                status,
            },
        );
    }

    fn lock(&self) -> MutexGuard<'_, State<K, R, F>> {
        // Un verrou « empoisonné » (panique d'un abonné) garde un état
        // cohérent : les modifications sont faites en une seule étape.
        self.shared
            .state
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
    }
}

/// Transmet un changement d'état à tous les abonnés, et oublie ceux qui ne
/// répondent plus.
fn publish<K, R, F>(state: &mut State<K, R, F>, snapshot: &JobSnapshot<K, R, F>) {
    state.subscribers.retain(|subscriber| subscriber(snapshot));
}

/// Ce qu'une tâche en cours peut faire : annoncer sa progression et vérifier
/// si l'utilisateur l'a annulée.
pub struct JobContext<K, R, F> {
    id: JobId,
    manager: JobManager<K, R, F>,
    cancellation: CancellationToken,
    last_report: Cell<Option<Instant>>,
}

impl<K, R, F> JobContext<K, R, F>
where
    K: Clone + Send + 'static,
    R: Clone + Send + 'static,
    F: Clone + Send + 'static,
{
    /// Identifiant de la tâche.
    #[must_use]
    pub fn id(&self) -> JobId {
        self.id
    }

    /// Jeton d'annulation de la tâche, à transmettre aux fonctions qui
    /// font le travail.
    #[must_use]
    pub fn cancellation(&self) -> &CancellationToken {
        &self.cancellation
    }

    /// Indique si l'utilisateur a demandé l'annulation.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Annonce la progression. Les annonces trop rapprochées sont ignorées
    /// (au plus une tous les 100 ms) : on peut l'appeler à chaque élément.
    pub fn report_progress(&self, progress: u64) {
        self.report(progress, None);
    }

    /// Annonce la progression et le nombre total d'éléments à traiter, pour
    /// un affichage « 1 234 sur 50 000 ». Mêmes règles que
    /// [`JobContext::report_progress`].
    pub fn report_progress_of(&self, progress: u64, total: u64) {
        self.report(progress, Some(total));
    }

    fn report(&self, progress: u64, total: Option<u64>) {
        let now = Instant::now();
        let too_soon = self
            .last_report
            .get()
            .is_some_and(|last| now.duration_since(last) < PROGRESS_INTERVAL);
        if too_soon {
            return;
        }
        self.last_report.set(Some(now));
        self.manager.update_progress(self.id, progress, total);
    }

    fn run<W>(self, work: W)
    where
        W: FnOnce(&Self) -> Result<R, JobError<F>>,
    {
        tracing::info!(job = self.id.0, "job started");
        let outcome = catch_unwind(AssertUnwindSafe(|| work(&self)));
        let status = match outcome {
            _ if self.is_cancelled() => JobStatus::Cancelled,
            Ok(Ok(result)) => JobStatus::Completed(result),
            Ok(Err(JobError::Cancelled)) => JobStatus::Cancelled,
            Ok(Err(JobError::Failed(failure))) => JobStatus::Failed(failure),
            Err(_) => {
                tracing::error!(job = self.id.0, "job panicked");
                JobStatus::Crashed
            }
        };
        tracing::info!(
            job = self.id.0,
            status = status_name(&status),
            "job finished"
        );
        self.manager.finish(self.id, status);
    }
}

fn status_name<R, F>(status: &JobStatus<R, F>) -> &'static str {
    match status {
        JobStatus::Running { .. } => "running",
        JobStatus::Completed(_) => "completed",
        JobStatus::Failed(_) => "failed",
        JobStatus::Cancelled => "cancelled",
        JobStatus::Crashed => "crashed",
    }
}
