import { useCallback, useEffect, useRef, useState } from "react";
import type { Dispatch, SetStateAction } from "react";

import {
  fetchRemembersFilterPerSource,
  fetchSourceFilter,
  saveSourceFilter,
} from "../services/filterPresets";
import type { GridSource } from "../services/library";
import { isFiltering, NO_FILTER_BAR } from "./gridFilter";
import type { FilterBarState } from "./gridFilter";

/** Attente après le dernier changement de filtre avant de le mémoriser. */
const SAVE_DELAY_MS = 500;

/** Clé d'une source, pour savoir à quelle source appartient le filtre. */
export function sourceKey(source: GridSource): string {
  switch (source.kind) {
    case "keyword":
      return `keyword-${String(source.keywordId)}`;
    case "folder":
      return `folder-${String(source.folderId)}`;
    case "collection":
      return `collection-${String(source.collectionId)}`;
    case "quickCollection":
      return "quickCollection";
    default:
      return source.kind;
  }
}

/**
 * Filtre de la barre au changement de source :
 * - cadenas fermé : il est gardé ;
 * - option « Mémoriser un filtre pour chaque dossier ou mot-clé » cochée :
 *   la nouvelle source retrouve le sien, mémorisé dans le catalogue à
 *   chaque changement (hors de Cmd+Z : c'est un réglage d'affichage) ;
 * - sinon, il revient à « Aucun ».
 *
 * @returns la fonction à appeler quand la source change.
 */
export function useSourceFilterMemory({
  source,
  filterBar,
  setFilterBar,
  isLocked,
}: {
  source: GridSource;
  filterBar: FilterBarState;
  setFilterBar: Dispatch<SetStateAction<FilterBarState>>;
  isLocked: boolean;
}): (next: GridSource) => void {
  const [remembers, setRemembers] = useState(false);
  // Source dont le filtre affiché est celui à mémoriser : `null` tant que
  // le filtre de la source affichée n'est pas retrouvé (sinon « Aucun »
  // remplacerait aussitôt le filtre mémorisé).
  const [owner, setOwner] = useState<string | null>(null);
  const latest = useRef(sourceKey(source));

  const restore = useCallback(
    (next: GridSource) => {
      const key = sourceKey(next);
      latest.current = key;
      fetchRemembersFilterPerSource()
        .then(async (isOn) => {
          setRemembers(isOn);
          const stored = isOn ? await fetchSourceFilter(next) : null;
          if (latest.current !== key) return;
          if (stored !== null) {
            setFilterBar({
              tabs: stored.tabs,
              filter: stored.filter,
              enabled: true,
            });
          }
          setOwner(key);
        })
        .catch((error: unknown) => {
          console.error("Failed to read the filter of the source", error);
        });
    },
    [setFilterBar],
  );

  // Au premier affichage : le filtre mémorisé de la source de départ.
  const initialSource = useRef(source);
  useEffect(() => {
    restore(initialSource.current);
  }, [restore]);

  const key = sourceKey(source);
  useEffect(() => {
    if (!remembers || isLocked || owner !== key) return;
    const timer = setTimeout(() => {
      const content =
        filterBar.tabs.length === 0 && !isFiltering(filterBar.filter)
          ? null
          : { tabs: [...filterBar.tabs], filter: filterBar.filter };
      saveSourceFilter(source, content).catch((error: unknown) => {
        console.error("Failed to remember the filter of the source", error);
      });
    }, SAVE_DELAY_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [remembers, isLocked, owner, key, source, filterBar]);

  return useCallback(
    (next: GridSource) => {
      if (isLocked) {
        // Le filtre suit la nouvelle source ; il n'y est mémorisé qu'une
        // fois le cadenas ouvert.
        latest.current = sourceKey(next);
        setOwner(latest.current);
        return;
      }
      setOwner(null);
      setFilterBar(NO_FILTER_BAR);
      restore(next);
    },
    [isLocked, restore, setFilterBar],
  );
}
