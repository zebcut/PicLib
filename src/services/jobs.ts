import { Channel } from "@tauri-apps/api/core";

import { commands } from "./bindings";
import type {
  FolderAnalysisSummary,
  JobFailure,
  JobKind,
  JobResult,
  JobUpdate,
  JobUpdateStatus,
} from "./bindings";

export type {
  FolderAnalysisSummary,
  JobFailure,
  JobKind,
  JobResult,
  JobUpdate,
  JobUpdateStatus,
};

/**
 * S'abonne aux mises à jour des tâches de fond : `onUpdate` reçoit aussitôt
 * l'état des tâches en cours, puis chaque changement.
 *
 * @throws si le cœur ne répond pas.
 */
export async function subscribeToJobs(
  onUpdate: (update: JobUpdate) => void,
): Promise<void> {
  const channel = new Channel<JobUpdate>();
  channel.onmessage = onUpdate;
  await commands.subscribeJobs(channel);
}

/**
 * Demande l'annulation d'une tâche ; renvoie `false` si elle est déjà
 * terminée.
 *
 * @throws si le cœur ne répond pas.
 */
export function cancelJob(jobId: number): Promise<boolean> {
  return commands.cancelJob(jobId);
}
