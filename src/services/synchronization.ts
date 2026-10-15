import { commands } from "./bindings";
import type {
  FolderSyncDto,
  ImportStartFailure,
  ImportStarted,
} from "./bindings";

export type { FolderSyncDto };

/**
 * Compare un dossier et son contenu sur le disque : fichiers nouveaux,
 * disparus et modifiés.
 *
 * @throws si le cœur ne répond pas, si le dossier est illisible ou si le
 *   catalogue est illisible.
 */
export async function fetchFolderSync(
  folderId: number,
): Promise<FolderSyncDto> {
  const result = await commands.getFolderSync(folderId);
  if (result.status === "error") {
    throw new Error(`Folder comparison unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Importe par référence les nouveaux fichiers d'un dossier, en tâche de
 * fond (ceux déjà au catalogue sont ignorés).
 *
 * @throws si le cœur ne répond pas.
 */
export async function importNewFiles(
  folderId: number,
): Promise<
  | { status: "ok"; data: ImportStarted }
  | { status: "error"; error: ImportStartFailure }
> {
  return commands.importNewFiles(folderId);
}

/**
 * Refait les vignettes et aperçus des fichiers modifiés depuis leur import
 * ; renvoie le nombre de fichiers concernés.
 *
 * @throws si le cœur ne répond pas.
 */
export function refreshReplacedFiles(mediaIds: number[]): Promise<number> {
  return commands.refreshReplacedFiles(mediaIds);
}
