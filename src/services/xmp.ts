import { commands } from "./bindings";
import type {
  XmpCheckDto,
  XmpWriteStartFailure,
  XmpWriteSummary,
} from "./bindings";
import { toEditResult } from "./edits";
import type { EditResult } from "./edits";

export type { XmpCheckDto, XmpWriteStartFailure, XmpWriteSummary };

/**
 * Écrit les métadonnées des médias indiqués dans leurs fichiers XMP, en tâche de fond : sa progression et son bilan arrivent par
 * l'abonnement aux tâches de fond.
 *
 * @param overwriteChanged remplacer aussi les fichiers modifiés par un autre
 *   logiciel (« Écraser avec le catalogue »).
 * @throws si le cœur ne répond pas.
 */
export function saveMetadataToFiles(
  mediaIds: number[],
  overwriteChanged = false,
): Promise<
  | { status: "ok"; data: null }
  | { status: "error"; error: XmpWriteStartFailure }
> {
  return commands.saveMetadataToFiles(mediaIds, overwriteChanged);
}

/** Vrai si l'écriture s'est passée sans rien à signaler : pas de bilan. */
export function isQuietXmpWrite(summary: XmpWriteSummary): boolean {
  return summary.changedElsewhere === 0 && summary.failed === 0;
}

/**
 * Cherche, dans le dossier et ses sous-dossiers, les XMP modifiés ou
 * supprimés par un autre logiciel.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function checkFolderSidecars(
  folderId: number,
): Promise<XmpCheckDto> {
  const result = await commands.checkFolderSidecars(folderId);
  if (result.status === "error") {
    throw new Error(`XMP check unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * « Lire depuis le fichier » : les XMP des photos remplacent ce que le
 * catalogue en sait, de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function readSidecarMetadata(
  mediaIds: number[],
): Promise<EditResult> {
  return toEditResult(await commands.readSidecarMetadata(mediaIds));
}
