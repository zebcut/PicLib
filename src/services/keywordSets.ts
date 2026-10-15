import { commands } from "./bindings";
import type {
  KeywordSetDto,
  KeywordSetFailure,
  KeywordSetSlotInput,
  KeywordSetSlotRefusal,
} from "./bindings";

export type { KeywordSetDto, KeywordSetSlotInput, KeywordSetSlotRefusal };

/** Opération refusée sur les jeux de mots-clés, avec la raison. */
export type KeywordSetRefusal = Exclude<
  KeywordSetFailure,
  { kind: "catalogUnavailable" }
>;

/**
 * Modification des jeux de mots-clés. Ce sont des réglages :
 * elles ne passent pas par l'historique (Cmd+Z).
 */
export type KeywordSetOperation =
  | { kind: "create"; name: string }
  | { kind: "rename"; setId: number; name: string }
  | { kind: "delete"; setId: number }
  | { kind: "activate"; setId: number }
  | { kind: "fill"; setId: number; slots: KeywordSetSlotInput[] };

/**
 * Tous les jeux de mots-clés, dans l'ordre de leur nom.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchKeywordSets(): Promise<KeywordSetDto[]> {
  const result = await commands.getKeywordSets();
  if (result.status === "error") {
    throw new Error(`Cannot read the keyword sets: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Crée, renomme, supprime, active ou remplit un jeu ; renvoie la raison
 * d'un refus, ou `null`.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function changeKeywordSets(
  operation: KeywordSetOperation,
): Promise<KeywordSetRefusal | null> {
  const result = await request(operation);
  if (result.status === "ok") return null;
  if (result.error.kind === "catalogUnavailable") {
    throw new Error(`Cannot change the keyword sets: ${operation.kind}`);
  }
  return result.error;
}

function request(operation: KeywordSetOperation) {
  switch (operation.kind) {
    case "create":
      return commands.createKeywordSet(operation.name);
    case "rename":
      return commands.renameKeywordSet(operation.setId, operation.name);
    case "delete":
      return commands.deleteKeywordSet(operation.setId);
    case "activate":
      return commands.activateKeywordSet(operation.setId);
    case "fill":
      return commands.fillKeywordSet(operation.setId, operation.slots);
  }
}
