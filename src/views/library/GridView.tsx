import { useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useEffect, useRef } from "react";
import type { MouseEvent, PointerEvent } from "react";
import { useTranslation } from "react-i18next";

import {
  AUTOSCROLL_ATTRIBUTE,
  isHovered,
  useDragStart,
  useDragState,
  useDropHandler,
} from "../../components/DragProvider";
import { counted } from "../../i18n/format";
import { isApplePlatform } from "../../shortcuts/registry";
import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import { useElementWidth } from "../../state/useElementWidth";
import type { GridItems } from "../../state/useGridItems";
import type { SelectionControls } from "../../state/useSelection";
import { GridCell } from "./GridCell";
import styles from "./GridView.module.css";

/** Rangées dessinées en plus, au-dessus et au-dessous de la partie visible. */
const OVERSCAN_ROWS = 3;

/**
 * Grille des vignettes, du plus récent au plus ancien. Seules les rangées
 * visibles sont dessinées (liste « virtualisée ») et seuls leurs éléments
 * sont demandés au cœur : le défilement reste fluide avec 50 000 éléments.
 *
 * À l'affichage, la grille défile jusqu'à l'élément sélectionné (retour de
 * la Loupe).
 *
 * @param cellSize taille souhaitée d'une case, en pixels ; elle est ajustée
 *   pour que les colonnes remplissent toute la largeur.
 * @param onOpen ouvre un élément en Loupe (double-clic).
 * @param onDropKeyword pose un mot-clé glissé depuis la liste des
 *   mots-clés : sur toute la sélection si la photo visée en fait partie,
 *   sinon sur elle seule (comme Lightroom).
 * @param onToggleTarget ajoute une photo à la collection cible, ou l'en
 *   retire (rond de la vignette) ; absent, pas de rond.
 * @param onToggleStack déplie ou replie la pile d'une photo (clic sur son
 *   indicateur).
 * @param onContextMenu ouvre le menu contextuel d'une photo (clic droit).
 * @param onXmpConflict propose de lire ou d'écraser le XMP d'une photo
 *   modifié par un autre logiciel (clic sur son icône).
 */
export function GridView({
  grid,
  cellSize,
  selection,
  onOpen,
  onDropKeyword,
  onToggleTarget,
  onToggleStack,
  onContextMenu,
  onXmpConflict,
}: {
  grid: GridItems;
  cellSize: number;
  selection: SelectionControls;
  onOpen: (mediaId: number) => void;
  onDropKeyword: (mediaIds: number[], keywordId: number) => void;
  onToggleTarget?: (mediaId: number) => void;
  onToggleStack?: (mediaId: number) => void;
  onContextMenu?: (mediaId: number) => void;
  onXmpConflict?: (mediaId: number) => void;
}) {
  const { activeId, ids: selectedIds } = selection.selection;
  const { t } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const width = useElementWidth(scrollRef);

  const count = grid.count ?? 0;
  const columns = Math.max(1, Math.floor(width / cellSize));
  const cellWidth = width > 0 ? width / columns : cellSize;
  const rowCount = Math.ceil(count / columns);

  // Avertissement propre au compilateur React, que PicLib n'utilise pas : les
  // fonctions renvoyées par useVirtualizer ne sont transmises à aucun
  // composant mémorisé.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => cellWidth,
    overscan: OVERSCAN_ROWS,
  });

  // Toutes les rangées changent de hauteur avec la taille des vignettes.
  useEffect(() => {
    virtualizer.measure();
  }, [virtualizer, cellWidth]);

  // Une seule fois, dès que la largeur est connue : amener l'élément
  // sélectionné à l'écran.
  const hasScrolledToSelection = useRef(false);
  useEffect(() => {
    if (hasScrolledToSelection.current || width === 0) return;
    hasScrolledToSelection.current = true;
    const index = activeId === null ? undefined : grid.indexOf(activeId);
    if (index !== undefined) {
      virtualizer.scrollToIndex(Math.floor(index / columns), {
        align: "center",
      });
    }
  }, [width, columns, activeId, grid, virtualizer]);

  const rows = virtualizer.getVirtualItems();
  const firstRow = rows[0]?.index ?? 0;
  const lastRow = rows[rows.length - 1]?.index ?? -1;
  const firstIndex = firstRow * columns;
  const lastIndex = Math.min(count, (lastRow + 1) * columns) - 1;

  const { ensureRange, version } = grid;
  useEffect(() => {
    ensureRange(firstIndex, lastIndex);
  }, [ensureRange, firstIndex, lastIndex, version]);

  /**
   * Rend actif l'élément situé `delta` cases plus loin, s'il existe : seul
   * sélectionné, ou en étendant la plage (`extend`, Maj+flèches).
   */
  const moveActive = (delta: number, extend: boolean) => {
    const current = activeId === null ? undefined : grid.indexOf(activeId);
    const target = current === undefined ? firstIndex : current + delta;
    if (target < 0 || target >= count) return;
    const item = grid.itemAt(target);
    if (item === undefined) return;
    if (extend && current !== undefined) selection.extendTo(target);
    else selection.selectOnly(item.id);
  };

  // L'élément actif reste à l'écran quand il change (flèches, avance
  // automatique) ; sans effet s'il est déjà visible.
  const activeIndex = activeId === null ? undefined : grid.indexOf(activeId);
  useEffect(() => {
    if (activeIndex === undefined || width === 0) return;
    virtualizer.scrollToIndex(Math.floor(activeIndex / columns), {
      align: "auto",
    });
  }, [activeIndex, columns, width, virtualizer]);

  useShortcuts({
    "selection.previous": () => {
      moveActive(-1, false);
    },
    "selection.next": () => {
      moveActive(1, false);
    },
    "selection.above": () => {
      moveActive(-columns, false);
    },
    "selection.below": () => {
      moveActive(columns, false);
    },
    "selection.extendPrevious": () => {
      moveActive(-1, true);
    },
    "selection.extendNext": () => {
      moveActive(1, true);
    },
    "selection.extendAbove": () => {
      moveActive(-columns, true);
    },
    "selection.extendBelow": () => {
      moveActive(columns, true);
    },
  });

  // Glisser des photos (vers un mot-clé) : toute la sélection si la photo
  // saisie en fait partie, sinon elle seule ; la sélection ne change pas.
  const startDrag = useDragStart();
  const drag = useDragState();
  const onCellPointerDown = useCallback(
    (mediaId: number, event: PointerEvent) => {
      const ids = selectedIds.has(mediaId) ? [...selectedIds] : [mediaId];
      startDrag(event, () => ({
        kind: "photos",
        mediaIds: ids,
        label: t("keywordList.dragPhotos", counted(ids.length)),
      }));
    },
    [selectedIds, startDrag, t],
  );
  useDropHandler(["media"], {
    canDrop: (payload) => payload.kind === "keyword",
    drop: (payload, target) => {
      if (payload.kind !== "keyword" || target.kind !== "media") return;
      const ids = selectedIds.has(target.mediaId)
        ? [...selectedIds]
        : [target.mediaId];
      onDropKeyword(ids, payload.keywordId);
    },
  });

  const onCellClick = useCallback(
    (mediaId: number, index: number, event: MouseEvent) => {
      selection.click(mediaId, index, {
        toggle: isApplePlatform() ? event.metaKey : event.ctrlKey,
        range: event.shiftKey,
      });
    },
    [selection],
  );

  return (
    <div
      {...AUTOSCROLL_ATTRIBUTE}
      ref={scrollRef}
      className={styles.scroller}
      role="listbox"
      aria-label={t("library.gridLabel")}
      aria-multiselectable
      onClick={(event) => {
        // Un clic à côté des vignettes désélectionne tout, comme Lightroom.
        if (event.target === event.currentTarget) selection.selectNone();
      }}
    >
      <div
        className={styles.content}
        style={{ height: virtualizer.getTotalSize() }}
      >
        {rows.map((row) => (
          <div
            key={row.key}
            className={styles.row}
            style={{
              height: cellWidth,
              transform: `translateY(${String(row.start)}px)`,
            }}
          >
            {Array.from({ length: columns }, (_, column) => {
              const index = row.index * columns + column;
              if (index >= count) return null;
              const item = grid.itemAt(index);
              return (
                <GridCell
                  key={column}
                  item={item}
                  index={index}
                  size={cellWidth}
                  isSelected={item !== undefined && selectedIds.has(item.id)}
                  isActive={item?.id === activeId}
                  isDropTarget={
                    item !== undefined &&
                    isHovered(drag, { kind: "media", mediaId: item.id })
                  }
                  onClick={onCellClick}
                  onPointerDown={onCellPointerDown}
                  onOpen={onOpen}
                  onToggleTarget={onToggleTarget}
                  onToggleStack={onToggleStack}
                  onContextMenu={onContextMenu}
                  onXmpConflict={onXmpConflict}
                />
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}
