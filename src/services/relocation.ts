import { commands } from "./bindings";
import type {
  EditOutcome,
  FolderRelocationPreview,
  RelocationFailure,
} from "./bindings";
import type { EditResult } from "./edits";

export type { FolderRelocationPreview };

/** Refus d'une relocalisation (format différent, déjà au catalogue…). */
export type RelocationRefusal = Exclude<
  RelocationFailure["kind"],
  "catalogUnavailable"
>;

/** Résultat d'une relocalisation : faite (annulable) ou refusée. */
export type RelocationResult =
  | { status: "ok"; result: EditResult }
  | { status: "refused"; refusal: RelocationRefusal };

function relocationResult(
  result:
    | { status: "ok"; data: EditOutcome }
    | { status: "error"; error: RelocationFailure },
): RelocationResult {
  if (result.status === "ok") {
    return { status: "ok", result: { status: "ok", outcome: result.data } };
  }
  return result.error.kind === "catalogUnavailable"
    ? {
        status: "ok",
        result: { status: "error", failure: { kind: "catalogUnavailable" } },
      }
    : { status: "refused", refusal: result.error.kind };
}

/**
 * Fait choisir le nouveau fichier d'une photo ; `null` si la fenêtre est
 * fermée.
 *
 * @throws si le cœur ne répond pas ou si l'emplacement est inutilisable.
 */
export async function chooseRelocatedFile(
  mediaId: number,
  title: string,
  filterName: string,
): Promise<string | null> {
  const result = await commands.chooseRelocatedFile(mediaId, title, filterName);
  if (result.status === "error") {
    throw new Error(`No file chosen: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Fait choisir le nouvel emplacement d'un dossier ; `null` si la fenêtre
 * est fermée.
 *
 * @throws si le cœur ne répond pas ou si l'emplacement est inutilisable.
 */
export async function chooseRelocatedFolder(
  title: string,
): Promise<string | null> {
  const result = await commands.chooseRelocatedFolder(title);
  if (result.status === "error") {
    throw new Error(`No folder chosen: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * La photo se trouve désormais à `path` (annulable).
 *
 * @throws si le cœur ne répond pas.
 */
export async function relocateMedia(
  mediaId: number,
  path: string,
): Promise<RelocationResult> {
  return relocationResult(await commands.relocateMedia(mediaId, path));
}

/**
 * Ce que donnerait la relocalisation d'un dossier en `path` ; un refus
 * s'il n'est pas possible.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchFolderRelocation(
  folderId: number,
  path: string,
): Promise<
  | { status: "ok"; preview: FolderRelocationPreview }
  | { status: "refused"; refusal: RelocationRefusal }
> {
  const result = await commands.getFolderRelocation(folderId, path);
  if (result.status === "ok") return { status: "ok", preview: result.data };
  if (result.error.kind === "catalogUnavailable") {
    throw new Error("Folder relocation unavailable");
  }
  return { status: "refused", refusal: result.error.kind };
}

/**
 * Le dossier se trouve désormais en `path` (annulable).
 *
 * @throws si le cœur ne répond pas.
 */
export async function relocateFolder(
  folderId: number,
  path: string,
): Promise<RelocationResult> {
  return relocationResult(await commands.relocateFolder(folderId, path));
}
