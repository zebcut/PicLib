import { useEffect, useState } from "react";

import { fetchSuggestedKeywords } from "../services/keywords";

/**
 * Délai, en millisecondes, avant de relire les suggestions : parcourir les
 * photos aux flèches ne déclenche pas un calcul par photo traversée.
 */
const READ_DELAY_MS = 120;

/** Mots-clés suggérés, lus dans le cœur. */
export type SuggestedKeywordsState =
  | { status: "loading" }
  | { status: "ready"; ids: number[] }
  | { status: "failed" };

/**
 * Mots-clés suggérés pour la photo active, relus quand elle
 * change ou quand `refreshKey` change (un mot-clé posé n'est plus
 * proposé). Les suggestions précédentes restent affichées pendant la
 * lecture.
 */
export function useSuggestedKeywords(
  activeId: number | null,
  refreshKey: number,
): SuggestedKeywordsState {
  const [state, setState] = useState<SuggestedKeywordsState>({
    status: "loading",
  });

  useEffect(() => {
    let isActive = true;
    const timer = setTimeout(() => {
      fetchSuggestedKeywords(activeId).then(
        (ids) => {
          if (isActive) setState({ status: "ready", ids });
        },
        (error: unknown) => {
          console.error("Failed to suggest keywords", error);
          if (isActive) setState({ status: "failed" });
        },
      );
    }, READ_DELAY_MS);
    return () => {
      isActive = false;
      clearTimeout(timer);
    };
  }, [activeId, refreshKey]);

  return state;
}
