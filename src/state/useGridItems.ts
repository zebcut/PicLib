import { useCallback, useEffect, useState } from "react";

import {
  fetchGridCount,
  fetchGridIds,
  fetchGridPage,
} from "../services/library";
import type { GridItem, GridQuery } from "../services/library";
import { GridPageCache } from "./gridPageCache";

/** Éléments de la grille (une source, filtrée et triée), chargés à la demande. */
export interface GridItems {
  /** Nombre total d'éléments ; `undefined` tant qu'il n'est pas connu. */
  count: number | undefined;
  /** Élément à cette position, s'il est déjà chargé. */
  itemAt: (index: number) => GridItem | undefined;
  /** Position d'un élément déjà chargé. */
  indexOf: (mediaId: number) => number | undefined;
  /** Charge ce qu'il faut pour afficher les positions `first` à `last`. */
  ensureRange: (first: number, last: number) => void;
  /**
   * Identifiants des positions `first` à `last` (incluses), dans l'ordre :
   * pris dans les pages chargées si possible, sinon demandés au cœur.
   */
  idsInRange: (first: number, last: number) => Promise<number[]>;
  /**
   * Change à chaque arrivée d'éléments ou relecture : les composants qui
   * affichent des éléments en dépendent pour se redessiner.
   */
  version: number;
}

/** Mémoire des pages d'une demande, et cette demande. */
interface QueryCache {
  query: GridQuery;
  cache: GridPageCache;
}

/**
 * Éléments de la grille. La source ne doit pas changer pendant la vie du
 * composant (lui donner une `key` par source) ; le filtre et le tri, si :
 * leurs pages sont alors toutes relues.
 *
 * @param query source, filtre et tri ; un nouvel objet relit tout.
 * @param refreshKey le nombre d'éléments et les pages affichées sont relus
 *   quand cette valeur change (import terminé, vignettes prêtes).
 */
export function useGridItems(query: GridQuery, refreshKey: number): GridItems {
  const [version, setVersion] = useState(0);
  const [count, setCount] = useState<number | undefined>(undefined);
  const newCache = (forQuery: GridQuery): QueryCache => ({
    query: forQuery,
    cache: new GridPageCache(
      (offset, limit) => fetchGridPage(forQuery, offset, limit),
      () => {
        setVersion((previous) => previous + 1);
      },
    ),
  });
  const [current, setCurrent] = useState(() => newCache(query));
  // Filtre ou tri changé : une mémoire neuve, sans les pages de l'ancien
  // ordre (ajustement pendant l'affichage, recommandé par React).
  if (current.query !== query) setCurrent(newCache(query));
  const { cache } = current;

  useEffect(() => {
    cache.attach();
    return () => {
      cache.detach();
    };
  }, [cache]);

  // La première relecture est inutile (rien n'est chargé), mais sans effet.
  useEffect(() => {
    cache.invalidate();
  }, [cache, refreshKey]);

  useEffect(() => {
    let isActive = true;
    fetchGridCount(current.query).then(
      (total) => {
        if (isActive) setCount(total);
      },
      (error: unknown) => {
        console.error("Failed to count the grid items", error);
      },
    );
    return () => {
      isActive = false;
    };
  }, [current, refreshKey]);

  const itemAt = useCallback((index: number) => cache.itemAt(index), [cache]);
  const indexOf = useCallback(
    (mediaId: number) => cache.indexOf(mediaId),
    [cache],
  );
  const ensureRange = useCallback(
    (first: number, last: number) => {
      cache.ensureRange(first, last);
    },
    [cache],
  );

  const idsInRange = useCallback(
    async (first: number, last: number): Promise<number[]> => {
      const loaded: number[] = [];
      for (let index = first; index <= last; index += 1) {
        const item = cache.itemAt(index);
        if (item === undefined) {
          return fetchGridIds(current.query, first, last - first + 1);
        }
        loaded.push(item.id);
      }
      return loaded;
    },
    [cache, current],
  );

  return { count, itemAt, indexOf, ensureRange, idsInRange, version };
}
