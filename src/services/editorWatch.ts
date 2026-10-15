import { commands } from "./bindings";
import type { EditInfoDto, FollowedPhotosDto } from "./bindings";

export type { EditInfoDto, FollowedPhotosDto };

/**
 * Photos envoyées à un éditeur dont les exports sont attendus, et dossiers
 * surveillés.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchFollowedPhotos(): Promise<FollowedPhotosDto> {
  const result = await commands.getFollowedPhotos();
  if (result.status === "error") {
    throw new Error(`Followed photos unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Arrête le suivi des photos indiquées, ou de toutes (`null`).
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function stopFollowing(
  mediaIds: number[] | null,
): Promise<FollowedPhotosDto> {
  const result = await commands.stopFollowing(mediaIds);
  if (result.status === "error") {
    throw new Error(`Cannot stop following: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Ce que le clic droit propose pour une photo envoyée à un éditeur : ne
 * plus la suivre, mettre à la corbeille une copie restée identique.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchEditInfo(mediaId: number): Promise<EditInfoDto> {
  const result = await commands.getEditInfo(mediaId);
  if (result.status === "error") {
    throw new Error(`Edit info unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Met à la corbeille une copie `-Modifié` restée identique à son original
 * (elle seule, même en couverture d'une pile repliée) ; vrai si la mise à
 * la corbeille est lancée, faux si la copie a été modifiée entre-temps.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function trashUnmodifiedCopy(mediaId: number): Promise<boolean> {
  const result = await commands.trashUnmodifiedCopy(mediaId);
  if (result.status === "ok") return true;
  if (result.error.kind === "notUnmodified") return false;
  throw new Error(`Cannot trash the copy: ${result.error.kind}`);
}
