import { useEffect, useMemo, useState } from "react";

import type { GridItems } from "../../state/useGridItems";
import { selectedOnly, wholeSource } from "./photoSequence";
import type { PhotoSequence } from "./photoSequence";

/**
 * Photos parcourues par le mode Tri ou la Comparaison : les photos
 * sélectionnées s'il y en a plusieurs, sinon tout l'ensemble affiché.
 * `undefined` tant qu'elles ne sont pas connues.
 *
 * Pour ranger une sélection dans l'ordre de la grille, les identifiants de
 * tout l'ensemble affiché sont demandés au cœur (2 ms pour 50 000 photos),
 * puis redemandés quand `refreshKey` change (import, retrait…).
 */
export function usePhotoSequence(
  grid: GridItems,
  selectedIds: ReadonlySet<number>,
  refreshKey: number,
): PhotoSequence | undefined {
  const { count, idsInRange } = grid;
  const isSelectionOnly = selectedIds.size > 1;
  const [allIds, setAllIds] = useState<readonly number[] | null>(null);

  useEffect(() => {
    if (!isSelectionOnly || count === undefined) return;
    let isActive = true;
    idsInRange(0, count - 1).then(
      (ids) => {
        if (isActive) setAllIds(ids);
      },
      (error: unknown) => {
        console.error("Failed to list the items to cull", error);
      },
    );
    return () => {
      isActive = false;
    };
  }, [isSelectionOnly, count, idsInRange, refreshKey]);

  return useMemo(() => {
    if (!isSelectionOnly) {
      return count === undefined ? undefined : wholeSource(count);
    }
    return allIds === null ? undefined : selectedOnly(allIds, selectedIds);
  }, [isSelectionOnly, count, allIds, selectedIds]);
}
