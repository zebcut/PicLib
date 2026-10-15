import { commands } from "./bindings";
import type {
  DiskDto,
  FolderDeletionDto,
  FolderDto,
  FolderMoveDto,
  FolderMoveResultDto,
  FolderPanelDto,
  FolderTrashResultDto,
  MoveRefusalDto,
  TrashCheckDto,
} from "./bindings";
import type { EditResult } from "./edits";

export type {
  DiskDto,
  FolderDeletionDto,
  FolderDto,
  FolderMoveDto,
  FolderMoveResultDto,
  MoveRefusalDto,
  FolderPanelDto,
  FolderTrashResultDto,
  TrashCheckDto,
};

/**
 * Contenu du panneau Dossiers : les disques et tous les dossiers du
 * catalogue, même vides.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchFolderPanel(): Promise<FolderPanelDto> {
  const result = await commands.getFolderPanel();
  if (result.status === "error") {
    throw new Error(`Cannot read the folder panel: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Résumé d'un dossier à supprimer, et si la corbeille est possible ;
 * `null` s'il n'est plus au catalogue.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchFolderDeletion(
  folderId: number,
): Promise<FolderDeletionDto | null> {
  const result = await commands.getFolderDeletion(folderId);
  if (result.status === "error") {
    throw new Error(`Cannot read the folder to delete: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Retire du catalogue un dossier, ses sous-dossiers et leurs médias
 * (fichiers intacts), de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function removeFolderFromCatalog(
  folderId: number,
): Promise<EditResult> {
  const result = await commands.removeFolderFromCatalog(folderId);
  return result.status === "ok"
    ? { status: "ok", outcome: result.data }
    : { status: "error", failure: result.error };
}

/**
 * Place un dossier dans la corbeille du système, s'il ne contient que des
 * fichiers connus du catalogue ; ses médias quittent le catalogue.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function trashFolder(
  folderId: number,
): Promise<FolderTrashResultDto> {
  const result = await commands.trashFolder(folderId);
  if (result.status === "error") {
    throw new Error(`Cannot trash the folder: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Résumé du déplacement d'un dossier dans un autre, avant confirmation ;
 * `null` si l'un des deux n'est plus au catalogue.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchFolderMove(
  folderId: number,
  destinationId: number,
): Promise<FolderMoveDto | null> {
  const result = await commands.getFolderMove(folderId, destinationId);
  if (result.status === "error") {
    throw new Error(`Cannot check the folder move: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Déplace un dossier dans un autre, sur le même disque, de façon annulable.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function moveFolder(
  folderId: number,
  destinationId: number,
): Promise<FolderMoveResultDto> {
  const result = await commands.moveFolder(folderId, destinationId);
  if (result.status === "error") {
    throw new Error(`Cannot move the folder: ${result.error.kind}`);
  }
  return result.data;
}
