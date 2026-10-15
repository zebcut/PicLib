import { commands } from "./bindings";
import type {
  CollectionDeletionDto,
  CollectionDto,
  CollectionFailure,
  CollectionKindDto,
  CollectionOperation,
  QuickCollectionDto,
  SmartRule,
} from "./bindings";
import type { EditResult } from "./edits";

export type {
  CollectionDeletionDto,
  CollectionDto,
  CollectionKindDto,
  CollectionOperation,
  QuickCollectionDto,
  SmartRule,
};

/** Refus d'un changement (nom vide ou pris, mauvaise sorte…). */
export type CollectionRefusal = Exclude<
  CollectionFailure["kind"],
  "catalogUnavailable"
>;

/** Résultat d'un changement : fait (annulable) ou refusé. */
export type CollectionChange =
  | { status: "ok"; result: EditResult; createdId: number | null }
  | { status: "refused"; refusal: CollectionRefusal };

/**
 * Arbre des collections, avec le nombre de photos de chacune.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchCollections(): Promise<CollectionDto[]> {
  const result = await commands.getCollections();
  if (result.status === "error") {
    throw new Error(`Collections unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Ce qu'emporterait la suppression d'une collection ou d'un dossier.
 *
 * @throws si le cœur ne répond pas, si le catalogue est illisible ou si la
 *   collection n'existe plus.
 */
export async function fetchCollectionDeletion(
  collectionId: number,
): Promise<CollectionDeletionDto> {
  const result = await commands.getCollectionDeletion(collectionId);
  if (result.status === "error") {
    throw new Error(`Collection deletion unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Crée, renomme, déplace, supprime une collection ou un dossier, ou y
 * ajoute ou en retire des photos, de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function changeCollections(
  operation: CollectionOperation,
): Promise<CollectionChange> {
  const result = await commands.changeCollections(operation);
  if (result.status === "ok") {
    return {
      status: "ok",
      result: { status: "ok", outcome: result.data.outcome },
      createdId: result.data.createdId,
    };
  }
  return result.error.kind === "catalogUnavailable"
    ? {
        status: "ok",
        result: { status: "error", failure: { kind: "catalogUnavailable" } },
        createdId: null,
      }
    : { status: "refused", refusal: result.error.kind };
}

/**
 * Nombre de photos de la collection rapide, et collection cible.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchQuickCollection(): Promise<QuickCollectionDto> {
  const result = await commands.getQuickCollection();
  if (result.status === "error") {
    throw new Error(`Quick collection unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Choisit la collection cible (`null` : la collection rapide) ; un réglage,
 * hors de Cmd+Z.
 *
 * @throws si le cœur ne répond pas, ou si la collection n'en est pas une.
 */
export async function setCollectionTarget(
  collectionId: number | null,
): Promise<void> {
  const result = await commands.setCollectionTarget(collectionId);
  if (result.status === "error") {
    throw new Error(`Collection target not set: ${result.error.kind}`);
  }
}
