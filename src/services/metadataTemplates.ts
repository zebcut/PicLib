import { commands } from "./bindings";
import type {
  MetadataTemplateDto,
  MetadataTemplateFailure,
  TemplateFieldDto,
  TemplateInputDto,
} from "./bindings";
import { toEditResult } from "./edits";
import type { EditResult } from "./edits";

export type {
  MetadataTemplateDto,
  MetadataTemplateFailure,
  TemplateFieldDto,
  TemplateInputDto,
};

/** Modèle refusé, avec la raison. */
export type MetadataTemplateRefusal = Exclude<
  MetadataTemplateFailure,
  { kind: "catalogUnavailable" }
>;

/**
 * Modification des modèles de métadonnées. Ce sont des
 * réglages : elles ne passent pas par l'historique (Cmd+Z).
 */
export type MetadataTemplateOperation =
  | { kind: "save"; templateId: number | null; template: TemplateInputDto }
  | { kind: "delete"; templateId: number };

/**
 * Tous les modèles, dans l'ordre de leur nom.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchMetadataTemplates(): Promise<MetadataTemplateDto[]> {
  const result = await commands.getMetadataTemplates();
  if (result.status === "error") {
    throw new Error(`Cannot read the templates: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Enregistre ou supprime un modèle ; renvoie la raison d'un refus, ou
 * `null`.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function changeMetadataTemplates(
  operation: MetadataTemplateOperation,
): Promise<MetadataTemplateRefusal | null> {
  const result =
    operation.kind === "save"
      ? await commands.saveMetadataTemplate(
          operation.templateId,
          operation.template,
        )
      : await commands.deleteMetadataTemplate(operation.templateId);
  if (result.status === "ok") return null;
  if (result.error.kind === "catalogUnavailable") {
    throw new Error(`Cannot change the templates: ${operation.kind}`);
  }
  return result.error;
}

/**
 * Applique un modèle aux médias indiqués, de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function applyMetadataTemplate(
  templateId: number,
  mediaIds: number[],
): Promise<EditResult> {
  return toEditResult(
    await commands.applyMetadataTemplate(templateId, mediaIds),
  );
}
