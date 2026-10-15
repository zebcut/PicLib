import { commands } from "./bindings";
import type {
  FileMoveRefusalDto,
  FileMoveResultDto,
  MediaMovePreviewDto,
  RenameDateFormat,
  RenamePart,
  RenamePreviewDto,
  RenameTemplate,
  RenameTemplateDto,
} from "./bindings";

export type {
  FileMoveRefusalDto,
  FileMoveResultDto,
  MediaMovePreviewDto,
  RenameDateFormat,
  RenamePart,
  RenamePreviewDto,
  RenameTemplate,
  RenameTemplateDto,
};

/** Résultat d'un déplacement : le résultat du cœur, ou un échec. */
export type FileMoveResult =
  { status: "ok"; result: FileMoveResultDto } | { status: "error" };

async function moved(
  request: Promise<
    | { status: "ok"; data: FileMoveResultDto }
    | { status: "error"; error: unknown }
  >,
): Promise<FileMoveResult> {
  const result = await request;
  if (result.status === "error") {
    console.error("Failed to move the files", result.error);
    return { status: "error" };
  }
  return { status: "ok", result: result.data };
}

/**
 * Résumé du déplacement de photos dans un dossier.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchMediaMove(
  mediaIds: number[],
  folderId: number,
): Promise<MediaMovePreviewDto> {
  const result = await commands.getMediaMove(mediaIds, folderId);
  if (result.status === "error") {
    throw new Error(`Photo move unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/** Déplace des photos dans un dossier (annulable). */
export function moveMedia(
  mediaIds: number[],
  folderId: number,
): Promise<FileMoveResult> {
  return moved(commands.moveMedia(mediaIds, folderId));
}

/** Crée un dossier et y déplace des photos (annulable). */
export function moveMediaToNewFolder(
  mediaIds: number[],
  parentId: number,
  name: string,
): Promise<FileMoveResult> {
  return moved(commands.moveMediaToNewFolder(mediaIds, parentId, name));
}

/** Renomme un dossier (annulable). */
export function renameFolder(
  folderId: number,
  name: string,
): Promise<FileMoveResult> {
  return moved(commands.renameFolder(folderId, name));
}

/**
 * Aperçu du renommage de photos avec un modèle.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchRenamePreview(
  mediaIds: number[],
  template: RenameTemplate,
): Promise<RenamePreviewDto> {
  const result = await commands.getRenamePreview(mediaIds, template);
  if (result.status === "error") {
    throw new Error(`Rename preview unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/** Renomme des photos avec un modèle (annulable). */
export function renameMedia(
  mediaIds: number[],
  template: RenameTemplate,
): Promise<FileMoveResult> {
  return moved(commands.renameMedia(mediaIds, template));
}

/**
 * Modèles de renommage enregistrés.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchRenameTemplates(): Promise<RenameTemplateDto[]> {
  const result = await commands.getRenameTemplates();
  if (result.status === "error") {
    throw new Error(`Rename templates unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Enregistre un modèle de renommage (remplace celui du même nom).
 *
 * @throws si le cœur ne répond pas ou si le catalogue ne peut pas être écrit.
 */
export async function saveRenameTemplate(
  name: string,
  template: RenameTemplate,
): Promise<void> {
  const result = await commands.saveRenameTemplate(name, template);
  if (result.status === "error") {
    throw new Error(`Rename template not saved: ${result.error.kind}`);
  }
}

/**
 * Supprime un modèle de renommage.
 *
 * @throws si le cœur ne répond pas ou si le catalogue ne peut pas être écrit.
 */
export async function deleteRenameTemplate(id: number): Promise<void> {
  const result = await commands.deleteRenameTemplate(id);
  if (result.status === "error") {
    throw new Error(`Rename template not deleted: ${result.error.kind}`);
  }
}
