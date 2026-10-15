//! Déclenchement d'une tâche de fond « par passages » (vignettes, lieux) :
//! une seule tourne à la fois, et une demande reçue pendant qu'elle tourne
//! lui fait refaire un passage à la fin plutôt que d'en lancer une seconde.

use std::sync::{Mutex, PoisonError};

/// État du déclenchement : la tâche tourne-t-elle, et faut-il refaire un
/// passage à la fin ?
#[derive(Debug, Default)]
pub struct PassScheduler(Mutex<SchedulerState>);

#[derive(Debug, Default)]
struct SchedulerState {
    running: bool,
    another_pass: bool,
}

impl PassScheduler {
    /// Réserve le droit de lancer la tâche. Si elle tourne déjà, demande
    /// seulement un passage de plus et renvoie `false`.
    pub fn begin(&self) -> bool {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        if state.running {
            state.another_pass = true;
            false
        } else {
            *state = SchedulerState {
                running: true,
                another_pass: false,
            };
            true
        }
    }

    /// Fin d'un passage : renvoie vrai s'il faut en refaire un.
    pub fn pass_finished(&self) -> bool {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        std::mem::take(&mut state.another_pass)
    }

    /// Fin de la tâche (ou échec de son lancement) : une nouvelle demande
    /// pourra la relancer.
    pub fn end(&self) {
        let mut state = self.0.lock().unwrap_or_else(PoisonError::into_inner);
        *state = SchedulerState::default();
    }

    /// Garde qui appelle [`PassScheduler::end`] quand la tâche se termine,
    /// y compris après une panique : sans elle, la tâche ne serait plus
    /// jamais relancée.
    #[must_use]
    pub fn end_guard(&self) -> EndGuard<'_> {
        EndGuard(self)
    }
}

/// Voir [`PassScheduler::end_guard`].
#[derive(Debug)]
pub struct EndGuard<'a>(&'a PassScheduler);

impl Drop for EndGuard<'_> {
    fn drop(&mut self) {
        self.0.end();
    }
}
