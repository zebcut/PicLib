import { useCallback, useEffect, useState } from "react";

import { changeKeywordSets, fetchKeywordSets } from "../services/keywordSets";
import type {
  KeywordSetDto,
  KeywordSetOperation,
  KeywordSetRefusal,
} from "../services/keywordSets";

/** Jeux de mots-clés, lus dans le cœur. */
export type KeywordSetsState =
  | { status: "loading" }
  | { status: "ready"; sets: KeywordSetDto[] }
  | { status: "failed" };

/** Les jeux de mots-clés, et de quoi les modifier. */
export interface KeywordSets {
  state: KeywordSetsState;
  /**
   * Modifie les jeux puis les relit ; renvoie la raison d'un refus, ou
   * `null`.
   *
   * @throws si le catalogue est illisible.
   */
  change: (operation: KeywordSetOperation) => Promise<KeywordSetRefusal | null>;
}

/**
 * Les jeux de mots-clés, relus quand `refreshKey` change (une
 * fusion ou son annulation déplace leurs cases) et après chaque
 * modification. Les jeux précédents restent affichés pendant la lecture.
 *
 * @param onKeywordsCreated appelé après avoir rempli un jeu : un mot-clé
 *   tapé dans une case a pu être créé.
 */
export function useKeywordSets(
  refreshKey: number,
  onKeywordsCreated: () => void,
): KeywordSets {
  const [state, setState] = useState<KeywordSetsState>({ status: "loading" });
  const [reloads, setReloads] = useState(0);

  useEffect(() => {
    let isActive = true;
    fetchKeywordSets().then(
      (sets) => {
        if (isActive) setState({ status: "ready", sets });
      },
      (error: unknown) => {
        console.error("Failed to read the keyword sets", error);
        if (isActive) setState({ status: "failed" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [refreshKey, reloads]);

  const change = useCallback(
    async (operation: KeywordSetOperation) => {
      const refusal = await changeKeywordSets(operation);
      if (refusal === null) {
        setReloads((previous) => previous + 1);
        if (operation.kind === "fill") onKeywordsCreated();
      }
      return refusal;
    },
    [onKeywordsCreated],
  );

  return { state, change };
}
