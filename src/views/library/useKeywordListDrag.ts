import { useEffect, useMemo } from "react";
import type { PointerEvent } from "react";

import {
  AUTOSCROLL_ATTRIBUTE,
  dropTargetAttributes,
  isHovered,
  useDragStart,
  useDragState,
  useDropHandler,
} from "../../components/DragProvider";
import { TREE_TWISTY_ATTRIBUTE } from "../../components/Tree";
import type { TreeRowProps } from "../../components/Tree";
import type { KeywordListItemDto } from "../../services/keywordList";

/** Délai, en millisecondes, avant qu'une branche survolée s'ouvre. */
const HOVER_EXPAND_MS = 700;

/** Ce qui se passe quand on dépose sur la liste des mots-clés. */
export interface KeywordListDropActions {
  /** Range `keywordId` sous `parentId` (`null` : à la racine). */
  move: (keywordId: number, parentId: number | null) => void;
  /** Pose le mot-clé sur des photos glissées depuis la grille. */
  apply: (mediaIds: number[], keywordId: number) => void;
  /** Déplie une branche survolée pendant un glisser. */
  expand: (keywordId: number) => void;
}

/**
 * Glisser-déposer dans la liste des mots-clés : un mot-clé se
 * glisse sur un autre (il s'y range, avec sa branche) ou sur le fond de la
 * liste (à la racine) ; des photos se glissent sur un mot-clé (il leur est
 * posé). Une branche repliée survolée s'ouvre au bout d'un instant.
 *
 * @returns les attributs d'une ligne de l'arbre, et ceux du fond de la
 *   liste.
 */
export function useKeywordListDrag(
  keywords: readonly KeywordListItemDto[],
  expanded: ReadonlySet<number>,
  actions: KeywordListDropActions,
  classes: { hovered: string | undefined; dragged: string | undefined },
) {
  const startDrag = useDragStart();
  const drag = useDragState();
  const parents = useMemo(
    () => new Map(keywords.map((keyword) => [keyword.id, keyword.parentId])),
    [keywords],
  );

  /** Vrai si `id` est `ancestor` ou se trouve dans sa branche. */
  const isWithin = (id: number, ancestor: number): boolean => {
    let current: number | null | undefined = id;
    // Au plus autant d'étapes que de mots-clés : protège d'une boucle.
    for (
      let step = 0;
      step <= parents.size && current !== null && current !== undefined;
      step += 1
    ) {
      if (current === ancestor) return true;
      current = parents.get(current);
    }
    return false;
  };

  useDropHandler(["keyword", "keywordRoot"], {
    canDrop: (payload, target) => {
      if (payload.kind === "folder" || payload.kind === "collection") {
        return false;
      }
      if (payload.kind === "photos") return target.kind === "keyword";
      if (target.kind === "keywordRoot") {
        return (parents.get(payload.keywordId) ?? null) !== null;
      }
      return (
        target.kind === "keyword" &&
        !isWithin(target.keywordId, payload.keywordId)
      );
    },
    drop: (payload, target) => {
      if (payload.kind === "folder" || payload.kind === "collection") return;
      if (payload.kind === "photos") {
        if (target.kind === "keyword") {
          actions.apply(payload.mediaIds, target.keywordId);
        }
        return;
      }
      if (target.kind === "keywordRoot") actions.move(payload.keywordId, null);
      else if (target.kind === "keyword") {
        actions.move(payload.keywordId, target.keywordId);
      }
    },
  });

  // Une branche repliée survolée s'ouvre au bout d'un instant.
  const hover = drag?.hover ?? null;
  const hoveredKeyword = hover?.kind === "keyword" ? hover.keywordId : null;
  const { expand } = actions;
  useEffect(() => {
    if (hoveredKeyword === null || expanded.has(hoveredKeyword)) return;
    const timer = setTimeout(() => {
      expand(hoveredKeyword);
    }, HOVER_EXPAND_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [hoveredKeyword, expanded, expand]);

  const rowProps = (keyword: KeywordListItemDto): TreeRowProps => {
    const target = { kind: "keyword", keywordId: keyword.id } as const;
    const isDragged =
      drag?.payload.kind === "keyword" && drag.payload.keywordId === keyword.id;
    return {
      ...dropTargetAttributes(target),
      className: isHovered(drag, target)
        ? classes.hovered
        : isDragged
          ? classes.dragged
          : undefined,
      onPointerDown: (event: PointerEvent<HTMLLIElement>) => {
        // La case à cocher, la flèche et le triangle gardent leur clic.
        const origin = event.target as HTMLElement;
        if (
          origin.closest(`input, button, [${TREE_TWISTY_ATTRIBUTE}]`) !== null
        ) {
          return;
        }
        startDrag(event, () => ({
          kind: "keyword",
          keywordId: keyword.id,
          label: keyword.name,
        }));
      },
    };
  };

  const rootProps = {
    ...dropTargetAttributes({ kind: "keywordRoot" }),
    ...AUTOSCROLL_ATTRIBUTE,
  };
  const isRootHovered = isHovered(drag, { kind: "keywordRoot" });

  return { rowProps, rootProps, isRootHovered };
}
