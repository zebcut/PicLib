import { useEffect, useState } from "react";

import { fetchKeywordPanel } from "../services/keywords";
import type { KeywordPanel, KeywordPresenceDto } from "../services/keywords";

/**
 * Délai, en millisecondes, avant de relire le panneau : parcourir les
 * photos aux flèches ne déclenche pas une lecture par photo traversée.
 */
const READ_DELAY_MS = 80;

/** Mots-clés de la sélection, lus dans le cœur. */
export type KeywordPanelState =
  | { status: "empty" }
  | { status: "loading" }
  | { status: "ready"; panel: KeywordPanel }
  | { status: "failed" };

/**
 * Mots-clés des photos sélectionnées, relus quand la sélection change ou
 * quand `refreshKey` change (modification, annulation…). Le contenu
 * précédent reste affiché pendant la lecture.
 */
export function useKeywordPanel(
  selectedIds: ReadonlySet<number>,
  refreshKey: number,
): KeywordPanelState {
  const [state, setState] = useState<KeywordPanelState>({
    status: "loading",
  });

  useEffect(() => {
    if (selectedIds.size === 0) return;
    let isActive = true;
    const timer = setTimeout(() => {
      fetchKeywordPanel([...selectedIds]).then(
        (panel) => {
          if (isActive) setState({ status: "ready", panel });
        },
        (error: unknown) => {
          console.error("Failed to read the keywords", error);
          if (isActive) setState({ status: "failed" });
        },
      );
    }, READ_DELAY_MS);
    return () => {
      isActive = false;
      clearTimeout(timer);
    };
  }, [selectedIds, refreshKey]);

  return selectedIds.size === 0 ? { status: "empty" } : state;
}

/**
 * Présence de chaque mot-clé saisi sur les photos sélectionnées : sur
 * toutes, sur une partie, ou sur aucune (parent seulement impliqué). Un
 * mot-clé absent de la table n'est sur aucune.
 */
export function selectionPresence(
  state: KeywordPanelState,
): ReadonlyMap<number, KeywordPresenceDto> {
  const byId = new Map<number, KeywordPresenceDto>();
  if (state.status === "ready") {
    for (const keyword of state.panel.withParents) {
      byId.set(keyword.id, keyword.entered);
    }
  }
  return byId;
}
