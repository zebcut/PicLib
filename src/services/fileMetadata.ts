import { commands } from "./bindings";
import type {
  FileMetadataStarted,
  FileMetadataStartFailure,
  FileMetadataSummary,
} from "./bindings";
import { toEditResult } from "./edits";
import type { EditResult } from "./edits";

export type { FileMetadataStartFailure, FileMetadataSummary };

/**
 * Lance la lecture des métadonnées des fichiers des médias indiqués : sa progression et son bilan arrivent par l'abonnement aux tâches
 * de fond. Rien n'est modifié avant `applyFileMetadata`.
 *
 * @throws si le cœur ne répond pas.
 */
export function readFileMetadata(
  mediaIds: number[],
): Promise<
  | { status: "ok"; data: FileMetadataStarted }
  | { status: "error"; error: FileMetadataStartFailure }
> {
  return commands.readFileMetadata(mediaIds);
}

/**
 * Complète les médias d'après la lecture `jobId`, de façon annulable :
 * seulement ce qui manque.
 *
 * @throws si le cœur ne répond pas.
 */
export async function applyFileMetadata(jobId: number): Promise<EditResult> {
  return toEditResult(await commands.applyFileMetadata(jobId));
}

/**
 * Oublie la lecture `jobId` sans l'appliquer.
 *
 * @throws si le cœur ne répond pas.
 */
export function discardFileMetadata(jobId: number): Promise<void> {
  return commands.discardFileMetadata(jobId);
}

/**
 * Vrai si la lecture s'applique sans confirmation : une seule photo, et
 * quelque chose à compléter (au-delà, un résumé est montré d'abord).
 */
export function appliesDirectly(summary: FileMetadataSummary): boolean {
  return summary.requested === 1 && summary.toComplete > 0;
}
