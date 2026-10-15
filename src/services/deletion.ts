import { commands } from "./bindings";
import type {
  GridFilter,
  GridSource,
  PhotoDeletionSummary,
  RejectedSummary,
  TrashStarted,
  TrashStartFailure,
  TrashSummary,
} from "./bindings";
import type { EditResult } from "./edits";

export type {
  PhotoDeletionSummary,
  RejectedSummary,
  TrashStartFailure,
  TrashSummary,
};

/**
 * Photos rejetées d'une source, filtrées : leur nombre et les premiers noms.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchRejected(
  source: GridSource,
  filter: GridFilter,
): Promise<RejectedSummary> {
  const result = await commands.getRejected(source, filter);
  if (result.status === "error") {
    throw new Error(`Rejected photos unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Retire du catalogue les photos rejetées d'une source, filtrées (fichiers
 * intacts),
 * de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function removeRejectedFromCatalog(
  source: GridSource,
  filter: GridFilter,
): Promise<EditResult> {
  const result = await commands.removeRejectedFromCatalog(source, filter);
  return result.status === "ok"
    ? { status: "ok", outcome: result.data }
    : { status: "error", failure: result.error };
}

/**
 * Lance la mise à la corbeille des photos rejetées d'une source, filtrées,
 * en tâche de fond ; sa progression et son bilan arrivent par l'abonnement aux
 * tâches.
 *
 * @throws si le cœur ne répond pas.
 */
export async function trashRejected(
  source: GridSource,
  filter: GridFilter,
): Promise<
  | { status: "ok"; data: TrashStarted }
  | { status: "error"; error: TrashStartFailure }
> {
  return commands.trashRejected(source, filter);
}

/**
 * Photos que Suppr emporterait (toute pile repliée sélectionnée comprise) :
 * leur nombre et les premiers noms.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchPhotoDeletion(
  mediaIds: number[],
): Promise<PhotoDeletionSummary> {
  const result = await commands.getPhotoDeletion(mediaIds);
  if (result.status === "error") {
    throw new Error(`Photos to delete unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Retire du catalogue les photos indiquées (fichiers intacts), de façon
 * annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function removePhotosFromCatalog(
  mediaIds: number[],
): Promise<EditResult> {
  const result = await commands.removePhotosFromCatalog(mediaIds);
  return result.status === "ok"
    ? { status: "ok", outcome: result.data }
    : { status: "error", failure: result.error };
}

/**
 * Lance la mise à la corbeille des photos indiquées et de leurs fichiers
 * annexes, en tâche de fond.
 *
 * @throws si le cœur ne répond pas.
 */
export async function trashPhotos(
  mediaIds: number[],
): Promise<
  | { status: "ok"; data: TrashStarted }
  | { status: "error"; error: TrashStartFailure }
> {
  return commands.trashPhotos(mediaIds);
}
