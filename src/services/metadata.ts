import { commands } from "./bindings";
import type {
  FieldState,
  FieldValueDto,
  GpsDto,
  MediaInfoDto,
  MetadataFieldDto,
  MetadataPanel,
} from "./bindings";
import { toEditResult } from "./edits";
import type { EditResult } from "./edits";

export type {
  FieldState,
  FieldValueDto,
  GpsDto,
  MediaInfoDto,
  MetadataFieldDto,
  MetadataPanel,
};

/**
 * Contenu du panneau Métadonnées : valeurs des champs pour les médias
 * sélectionnés (« valeurs mixtes » si elles diffèrent) et informations de
 * la photo active.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchMetadataPanel(
  activeId: number,
  mediaIds: number[],
): Promise<MetadataPanel> {
  const result = await commands.getMetadataPanel(activeId, mediaIds);
  if (result.status === "error") {
    throw new Error(`Cannot read the metadata: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Donne une valeur à un champ de métadonnées des médias indiqués, de façon
 * annulable ; un texte vide vide le champ.
 *
 * @throws si le cœur ne répond pas.
 */
export async function setMetadata(
  mediaIds: number[],
  field: MetadataFieldDto,
  value: string,
): Promise<EditResult> {
  return toEditResult(await commands.setMetadata(mediaIds, field, value));
}
