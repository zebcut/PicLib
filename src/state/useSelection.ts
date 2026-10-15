import { useCallback, useMemo, useState } from "react";

import {
  activate,
  EMPTY_SELECTION,
  keepVisible,
  rangeBounds,
  selectAll,
  selectMany,
  selectOnly,
  selectRange,
  toggle,
} from "./selection";
import type { Selection } from "./selection";
import type { GridItems } from "./useGridItems";

/** Touches de modification d'un clic sur un élément. */
export interface ClickModifiers {
  /** Cmd+clic (Ctrl+clic sous Windows) : ajouter ou retirer. */
  toggle: boolean;
  /** Maj+clic : sélectionner une plage. */
  range: boolean;
}

/** Sélection de la bibliothèque et ses actions. */
export interface SelectionControls {
  selection: Selection;
  /** Clic sur un élément, avec ou sans touches de modification. */
  click: (mediaId: number, index: number, modifiers: ClickModifiers) => void;
  /** Sélectionne un seul élément et le rend actif (flèches, Loupe). */
  selectOnly: (mediaId: number) => void;
  /**
   * Rend actif un élément déjà sélectionné, en gardant la sélection (mode
   * Tri) ; un élément non sélectionné est ignoré.
   */
  activate: (mediaId: number) => void;
  /** Étend la plage jusqu'à la position `index` (Maj+flèches). */
  extendTo: (index: number) => void;
  /** Sélectionne ces éléments, le premier actif (groupe de la carte). */
  selectIds: (ids: readonly number[]) => void;
  /** Sélectionne tous les éléments de la source (Cmd+A). */
  selectAll: () => void;
  /** Ne sélectionne rien (Cmd+D). */
  selectNone: () => void;
  /**
   * Ne garde que les éléments encore affichés (voir `keepVisible`), après
   * un changement de filtre ou une modification qui en masque.
   */
  keepVisible: (
    visibleIds: readonly number[],
    fallbackIndex: number | null,
  ) => void;
}

/**
 * Sélection partagée par la Grille et la Loupe. Les plages et « tout »
 * peuvent inclure des éléments pas encore chargés : leurs identifiants sont
 * alors demandés au cœur.
 */
export function useSelection(grid: GridItems): SelectionControls {
  const [selection, setSelection] = useState<Selection>(EMPTY_SELECTION);
  const { indexOf, itemAt, idsInRange, count } = grid;

  /** Sélectionne la plage du point de départ à `index`. */
  const anchorIndex = selection.anchorIndex;
  const selectRangeTo = useCallback(
    (index: number, keepExisting: boolean) => {
      const target = itemAt(index);
      if (target === undefined) return;
      const { first, last } = rangeBounds(anchorIndex, index);
      // En attendant les identifiants de la plage, l'élément visé est déjà
      // sélectionné et actif.
      setSelection((current) => selectRange(current, [], target.id, true));
      idsInRange(first, last).then(
        (ids) => {
          setSelection((latest) =>
            selectRange(latest, ids, target.id, keepExisting),
          );
        },
        (error: unknown) => {
          console.error("Failed to select a range", error);
        },
      );
    },
    [anchorIndex, idsInRange, itemAt],
  );

  const click = useCallback(
    (mediaId: number, index: number, modifiers: ClickModifiers) => {
      if (modifiers.range) {
        selectRangeTo(index, modifiers.toggle);
      } else if (modifiers.toggle) {
        setSelection((current) => toggle(current, mediaId, index));
      } else {
        setSelection(selectOnly(mediaId, index));
      }
    },
    [selectRangeTo],
  );

  const selectOnlyId = useCallback(
    (mediaId: number) => {
      setSelection(selectOnly(mediaId, indexOf(mediaId) ?? 0));
    },
    [indexOf],
  );

  const activateId = useCallback(
    (mediaId: number) => {
      setSelection((current) =>
        activate(current, mediaId, indexOf(mediaId) ?? 0),
      );
    },
    [indexOf],
  );

  const selectIds = useCallback(
    (ids: readonly number[]) => {
      const first = ids[0];
      setSelection(
        selectMany(ids, first === undefined ? null : (indexOf(first) ?? null)),
      );
    },
    [indexOf],
  );

  const extendTo = useCallback(
    (index: number) => {
      selectRangeTo(index, false);
    },
    [selectRangeTo],
  );

  const selectEverything = useCallback(() => {
    if (count === undefined || count === 0) return;
    idsInRange(0, count - 1).then(
      (ids) => {
        setSelection((current) => selectAll(current, ids));
      },
      (error: unknown) => {
        console.error("Failed to select everything", error);
      },
    );
  }, [count, idsInRange]);

  const selectNone = useCallback(() => {
    setSelection(EMPTY_SELECTION);
  }, []);

  const keepOnlyVisible = useCallback(
    (visibleIds: readonly number[], fallbackIndex: number | null) => {
      setSelection((current) =>
        keepVisible(current, visibleIds, fallbackIndex),
      );
    },
    [],
  );

  return useMemo(
    () => ({
      selection,
      click,
      selectOnly: selectOnlyId,
      activate: activateId,
      extendTo,
      selectIds,
      selectAll: selectEverything,
      selectNone,
      keepVisible: keepOnlyVisible,
    }),
    [
      selection,
      click,
      selectOnlyId,
      activateId,
      extendTo,
      selectIds,
      selectEverything,
      selectNone,
      keepOnlyVisible,
    ],
  );
}
