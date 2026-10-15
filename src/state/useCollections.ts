import { useEffect, useState } from "react";

import {
  fetchCollections,
  fetchQuickCollection,
} from "../services/collections";
import type {
  CollectionDto,
  QuickCollectionDto,
} from "../services/collections";

/** Contenu du panneau Collections, lu dans le cœur. */
export type CollectionsState =
  | { status: "loading" }
  | {
      status: "ready";
      collections: CollectionDto[];
      /** Collection rapide et collection cible. */
      quick: QuickCollectionDto;
    }
  | { status: "failed" };

/**
 * Collections, dossiers de collections et collection rapide, relus quand
 * `refreshKey` change (chaque modification, dont les leurs et leur
 * annulation) ou que la cible change (`targetKey`). Le contenu précédent
 * reste affiché pendant la lecture.
 */
export function useCollections(
  refreshKey: number,
  targetKey = 0,
): CollectionsState {
  const [state, setState] = useState<CollectionsState>({ status: "loading" });

  useEffect(() => {
    let isActive = true;
    Promise.all([fetchCollections(), fetchQuickCollection()]).then(
      ([collections, quick]) => {
        if (isActive) setState({ status: "ready", collections, quick });
      },
      (error: unknown) => {
        console.error("Failed to read the collections", error);
        if (isActive) setState({ status: "failed" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [refreshKey, targetKey]);

  return state;
}
