import type { KeywordListItemDto } from "../../services/keywordList";
import type { KeywordPanelState } from "../../state/useKeywordPanel";
import { selectionPresence } from "../../state/useKeywordPanel";
import type { KeywordSetsState } from "../../state/useKeywordSets";
import {
  KEYWORD_SET_SLOTS,
  keywordSetShortcut,
} from "../../shortcuts/registry";
import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import type { ShortcutHandlers } from "../../shortcuts/ShortcutProvider";

/**
 * Jeu de mots-clés : Option+1 à Option+9 posent le mot-clé de
 * la case sur les photos sélectionnées, ou l'en retirent s'il est déjà sur
 * toutes. Une case vide, ou dont le mot-clé n'existe plus, ne fait rien.
 *
 * @returns la même action pour un mot-clé donné (clic sur un bouton du
 *   jeu).
 */
export function useKeywordSetShortcuts({
  selectedIds,
  selection,
  sets,
  keywords,
  add,
  remove,
}: {
  selectedIds: ReadonlySet<number>;
  selection: KeywordPanelState;
  sets: KeywordSetsState;
  keywords: readonly KeywordListItemDto[];
  add: (mediaIds: number[], keywordId: number) => void;
  remove: (mediaIds: number[], keywordId: number) => void;
}): (keywordId: number) => void {
  const toggle = (keywordId: number) => {
    if (selectedIds.size === 0) return;
    const mediaIds = [...selectedIds];
    if (selectionPresence(selection).get(keywordId) === "all") {
      remove(mediaIds, keywordId);
    } else {
      add(mediaIds, keywordId);
    }
  };

  const active =
    sets.status === "ready" ? sets.sets.find((set) => set.active) : undefined;
  const handlers: ShortcutHandlers = {};
  for (const slot of KEYWORD_SET_SLOTS) {
    handlers[keywordSetShortcut(slot)] = () => {
      const keywordId = active?.slots[slot - 1] ?? null;
      if (
        keywordId !== null &&
        keywords.some((keyword) => keyword.id === keywordId)
      ) {
        toggle(keywordId);
      }
    };
  }
  useShortcuts(handlers);

  return toggle;
}
