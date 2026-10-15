import { commands } from "./bindings";
import type {
  KeywordFailure,
  KeywordPanel,
  KeywordPresenceDto,
  KeywordSuggestionDto,
  SelectionKeywordDto,
} from "./bindings";
import { toEditResult } from "./edits";
import type { EditResult } from "./edits";

export type {
  KeywordFailure,
  KeywordPanel,
  KeywordPresenceDto,
  KeywordSuggestionDto,
  SelectionKeywordDto,
};

/**
 * Résultat d'une saisie de mots-clés : modification faite, ou raison du
 * refus (nom ambigu, trop long…) à afficher sous le champ.
 */
export type AddKeywordsResult =
  | { status: "ok"; result: EditResult }
  | {
      status: "refused";
      failure: Exclude<KeywordFailure, { kind: "catalogUnavailable" }>;
    };

/**
 * Mots-clés des médias sélectionnés avec leurs parents, et ceux qui seront
 * exportés.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchKeywordPanel(
  mediaIds: number[],
): Promise<KeywordPanel> {
  const result = await commands.getKeywordPanel(mediaIds);
  if (result.status === "error") {
    throw new Error(`Cannot read the keywords: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Ajoute aux médias les mots-clés d'une saisie (`Lieux > Paris, Été`), de
 * façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function addKeywords(
  mediaIds: number[],
  entry: string,
): Promise<AddKeywordsResult> {
  const result = await commands.addKeywords(mediaIds, entry);
  if (result.status === "ok") {
    return { status: "ok", result: { status: "ok", outcome: result.data } };
  }
  const failure = result.error;
  return failure.kind === "catalogUnavailable"
    ? {
        status: "ok",
        result: { status: "error", failure: { kind: "catalogUnavailable" } },
      }
    : { status: "refused", failure };
}

/**
 * Retire un mot-clé des médias, de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function removeKeyword(
  mediaIds: number[],
  keywordId: number,
): Promise<EditResult> {
  return toEditResult(await commands.removeKeyword(mediaIds, keywordId));
}

/**
 * Suggestions pour le mot-clé en cours de saisie (ce qui suit la dernière
 * virgule, par exemple `Lieux > Pays-Bas > Ze`).
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function suggestKeywords(
  token: string,
): Promise<KeywordSuggestionDto[]> {
  const result = await commands.suggestKeywords(token);
  if (result.status === "error") {
    throw new Error(`Cannot suggest keywords: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Mots-clés suggérés pour la photo active (sans photo active : les
 * mots-clés récents), du plus probable au moins probable.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchSuggestedKeywords(
  mediaId: number | null,
): Promise<number[]> {
  const result = await commands.getSuggestedKeywords(mediaId);
  if (result.status === "error") {
    throw new Error(`Cannot suggest keywords: ${result.error.kind}`);
  }
  return result.data;
}
