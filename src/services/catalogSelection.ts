import { commands } from "./bindings";
import type {
  CatalogSelectionFailure,
  CreateCatalogOutcome,
  OpenCatalogOutcome,
} from "./bindings";

export type {
  CatalogSelectionFailure,
  CreateCatalogOutcome,
  OpenCatalogOutcome,
};

/** Réponse du cœur à une demande de changement de catalogue. */
export type SelectionResult<T> =
  | { status: "ok"; data: T }
  | { status: "error"; error: CatalogSelectionFailure };

/*
 * En cas de succès, le cœur redémarre l'application sur le nouveau catalogue :
 * la promesse ne se résout alors jamais. Elle se résout seulement si
 * l'utilisateur annule, ou si le changement est impossible.
 */

/**
 * Fait choisir un dossier de catalogue existant, puis bascule dessus.
 *
 * @param dialogTitle titre traduit de la fenêtre de sélection du système.
 * @throws si le cœur ne répond pas.
 */
export function openCatalog(
  dialogTitle: string,
): Promise<SelectionResult<OpenCatalogOutcome>> {
  return commands.openCatalog(dialogTitle);
}

/**
 * Fait choisir l'emplacement et le nom d'un nouveau catalogue, le crée, puis
 * bascule dessus (après confirmation si l'emplacement est synchronisé).
 *
 * @param dialogTitle titre traduit de la fenêtre de sélection du système.
 * @param defaultName nom proposé pour le nouveau dossier.
 * @throws si le cœur ne répond pas.
 */
export function createCatalog(
  dialogTitle: string,
  defaultName: string,
): Promise<SelectionResult<CreateCatalogOutcome>> {
  return commands.createCatalog(dialogTitle, defaultName);
}

/**
 * Confirme la création d'un catalogue dans un emplacement synchronisé.
 *
 * @param addNosyncSuffix ajouter `.nosync` au nom (conseillé).
 * @throws si le cœur ne répond pas.
 */
export function confirmCatalogCreation(
  addNosyncSuffix: boolean,
): Promise<SelectionResult<null>> {
  return commands.confirmCatalogCreation(addNosyncSuffix);
}
