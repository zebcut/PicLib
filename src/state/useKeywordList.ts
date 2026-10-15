import { useCallback, useEffect, useState } from "react";

import { fetchKeywordList } from "../services/keywordList";
import type { KeywordListItemDto } from "../services/keywordList";

/** Liste de tous les mots-clés, lue dans le cœur. */
export type KeywordListState =
  | { status: "loading" }
  | { status: "ready"; keywords: KeywordListItemDto[] }
  | { status: "failed" };

/** La liste des mots-clés, et de quoi la relire. */
export interface KeywordList {
  state: KeywordListState;
  /** Relit la liste (après une création, qui ne passe pas par l'historique). */
  reload: () => void;
}

/**
 * Tous les mots-clés avec le nombre de photos de chacun, relus quand
 * `refreshKey` change (modification, annulation, import…) ou sur demande.
 * La liste précédente reste affichée pendant la lecture.
 */
export function useKeywordList(refreshKey: number): KeywordList {
  const [state, setState] = useState<KeywordListState>({ status: "loading" });
  const [reloads, setReloads] = useState(0);

  useEffect(() => {
    let isActive = true;
    fetchKeywordList().then(
      (keywords) => {
        if (isActive) setState({ status: "ready", keywords });
      },
      (error: unknown) => {
        console.error("Failed to read the keyword list", error);
        if (isActive) setState({ status: "failed" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [refreshKey, reloads]);

  const reload = useCallback(() => {
    setReloads((previous) => previous + 1);
  }, []);

  return { state, reload };
}
