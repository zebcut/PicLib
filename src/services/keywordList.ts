import { commands } from "./bindings";
import type {
  EditFailure,
  EditOutcome,
  KeywordListFailure,
  KeywordListItemDto,
} from "./bindings";
import { toEditResult } from "./edits";
import type { EditResult } from "./edits";

export type { KeywordListFailure, KeywordListItemDto };

/** Opération refusée sur la liste des mots-clés, avec la raison. */
export type KeywordListRefusal = Exclude<
  KeywordListFailure,
  { kind: "catalogUnavailable" }
>;

/**
 * Résultat d'une opération sur la liste : modification faite (ou échec du
 * catalogue), ou refus à expliquer (nom déjà pris…).
 */
export type KeywordListResult =
  | { status: "ok"; result: EditResult }
  | { status: "refused"; refusal: KeywordListRefusal };

/** Modification annulable de la liste des mots-clés. */
export type KeywordListOperation =
  | { kind: "rename"; keywordId: number; name: string }
  | { kind: "move"; keywordId: number; parentId: number | null }
  | { kind: "merge"; keywordId: number; intoId: number }
  | { kind: "delete"; keywordId: number }
  | { kind: "deleteUnused" }
  | { kind: "export"; keywordId: number; include: boolean };

/**
 * Tous les mots-clés, avec le nombre de photos de leur branche.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchKeywordList(): Promise<KeywordListItemDto[]> {
  const result = await commands.getKeywordList();
  if (result.status === "error") {
    throw new Error(`Cannot read the keyword list: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Pose un mot-clé sur les médias (case cochée), de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function addKeyword(
  mediaIds: number[],
  keywordId: number,
): Promise<EditResult> {
  return toEditResult(await commands.addKeyword(mediaIds, keywordId));
}

/**
 * Crée un mot-clé sous `parentId` (`null` : à la racine) ; renvoie son
 * identifiant, ou la raison du refus. Pas d'annulation : il reste dans la
 * liste, comme à la saisie.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function createKeyword(
  parentId: number | null,
  name: string,
  includeOnExport: boolean,
): Promise<
  | { status: "ok"; id: number }
  | { status: "refused"; refusal: KeywordListRefusal }
> {
  const result = await commands.createKeyword(parentId, name, includeOnExport);
  if (result.status === "ok") return { status: "ok", id: result.data };
  if (result.error.kind === "catalogUnavailable") {
    throw new Error("Cannot create the keyword: catalogUnavailable");
  }
  return { status: "refused", refusal: result.error };
}

/**
 * Renomme, déplace, fusionne, supprime un mot-clé, ou coche « inclure à
 * l'export »,
 * de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function changeKeywordList(
  operation: KeywordListOperation,
): Promise<KeywordListResult> {
  return toKeywordListResult(await request(operation));
}

function request(operation: KeywordListOperation) {
  switch (operation.kind) {
    case "rename":
      return commands.renameKeyword(operation.keywordId, operation.name);
    case "move":
      return commands.moveKeyword(operation.keywordId, operation.parentId);
    case "merge":
      return commands.mergeKeyword(operation.keywordId, operation.intoId);
    case "delete":
      return commands.deleteKeyword(operation.keywordId);
    case "deleteUnused":
      return commands.deleteUnusedKeywords();
    case "export":
      return commands.setKeywordExport(operation.keywordId, operation.include);
  }
}

function toKeywordListResult(
  result:
    | { status: "ok"; data: EditOutcome }
    | { status: "error"; error: KeywordListFailure },
): KeywordListResult {
  if (result.status === "ok") {
    return { status: "ok", result: { status: "ok", outcome: result.data } };
  }
  if (result.error.kind === "catalogUnavailable") {
    const failure: EditFailure = { kind: "catalogUnavailable" };
    return { status: "ok", result: { status: "error", failure } };
  }
  return { status: "refused", refusal: result.error };
}
