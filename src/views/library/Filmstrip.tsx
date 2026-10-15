import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useCallback, useEffect, useRef } from "react";
import type { MouseEvent, PointerEvent } from "react";
import { useTranslation } from "react-i18next";

import { useDragStart } from "../../components/DragProvider";
import { ResizeHandle } from "../../components/ResizeHandle";
import { counted, formatCaptureTime } from "../../i18n/format";
import type { GridItem } from "../../services/library";
import { isApplePlatform } from "../../shortcuts/registry";
import type { GridItems } from "../../state/useGridItems";
import type { SelectionControls } from "../../state/useSelection";
import { useStoredNumber } from "../../state/useStoredNumber";
import styles from "./Filmstrip.module.css";
import { StackIndicator, Thumbnail } from "./GridCell";
import gridStyles from "./GridView.module.css";
import { TriageBadges } from "./TriageBadges";

/** Hauteur de la filmstrip, en pixels, retenue d'une session à l'autre. */
export const FILMSTRIP_HEIGHT = {
  storageKey: "piclib.filmstrip.height",
  default: 96,
  min: 60,
  max: 220,
};

/** Vignettes dessinées en plus, de part et d'autre de la partie visible. */
const OVERSCAN_CELLS = 5;

/**
 * Filmstrip : les éléments de la grille (même source, mêmes
 * filtres, même tri) en une rangée au bas de la vue, avec la sélection
 * partagée. Seules les vignettes visibles sont dessinées ; l'élément actif
 * reste à l'écran. La hauteur se règle en glissant le bord supérieur.
 *
 * @param onOpen ouvre un élément en Loupe (double-clic).
 */
export function Filmstrip({
  grid,
  selection,
  onOpen,
}: {
  grid: GridItems;
  selection: SelectionControls;
  onOpen: (mediaId: number) => void;
}) {
  const { t } = useTranslation();
  const [height, setHeight] = useStoredNumber(
    FILMSTRIP_HEIGHT.storageKey,
    FILMSTRIP_HEIGHT.default,
    FILMSTRIP_HEIGHT.min,
    FILMSTRIP_HEIGHT.max,
  );
  const scrollRef = useRef<HTMLDivElement>(null);
  const { activeId, ids: selectedIds } = selection.selection;
  const count = grid.count ?? 0;

  // Avertissement propre au compilateur React, que PicLib n'utilise pas : les
  // fonctions renvoyées par useVirtualizer ne sont transmises à aucun
  // composant mémorisé.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count,
    horizontal: true,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => height,
    overscan: OVERSCAN_CELLS,
  });

  // Toutes les vignettes changent de largeur avec la hauteur.
  useEffect(() => {
    virtualizer.measure();
  }, [virtualizer, height]);

  const cells = virtualizer.getVirtualItems();
  const firstIndex = cells[0]?.index ?? 0;
  const lastIndex = cells[cells.length - 1]?.index ?? -1;
  const { ensureRange, version } = grid;
  useEffect(() => {
    ensureRange(firstIndex, lastIndex);
  }, [ensureRange, firstIndex, lastIndex, version]);

  // L'élément actif reste à l'écran quand il change ; sans effet s'il est
  // déjà visible.
  const activeIndex = activeId === null ? undefined : grid.indexOf(activeId);
  useEffect(() => {
    if (activeIndex !== undefined) {
      virtualizer.scrollToIndex(activeIndex, { align: "auto" });
    }
  }, [activeIndex, virtualizer]);

  // Glisser des photos (vers une collection, un mot-clé) : toute la
  // sélection si la photo saisie en fait partie, sinon elle seule.
  const startDrag = useDragStart();
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

  const onCellClick = (mediaId: number, index: number, event: MouseEvent) => {
    selection.click(mediaId, index, {
      toggle: isApplePlatform() ? event.metaKey : event.ctrlKey,
      range: event.shiftKey,
    });
  };

  return (
    <div className={styles.filmstrip}>
      <ResizeHandle
        label={t("library.filmstrip.resize")}
        orientation="horizontal"
        direction={-1}
        value={height}
        min={FILMSTRIP_HEIGHT.min}
        max={FILMSTRIP_HEIGHT.max}
        defaultValue={FILMSTRIP_HEIGHT.default}
        onChange={setHeight}
      />
      <div
        ref={scrollRef}
        className={styles.scroller}
        style={{ height }}
        role="list"
        aria-label={t("library.filmstrip.label")}
      >
        <div
          className={styles.content}
          style={{ width: virtualizer.getTotalSize(), height }}
        >
          {cells.map((cell) => (
            <div
              key={cell.key}
              role="listitem"
              className={styles.slot}
              style={{
                width: height,
                height,
                transform: `translateX(${String(cell.start)}px)`,
              }}
            >
              <FilmstripCell
                item={grid.itemAt(cell.index)}
                index={cell.index}
                isSelected={selectedIds.has(grid.itemAt(cell.index)?.id ?? -1)}
                isActive={
                  activeId !== null && grid.itemAt(cell.index)?.id === activeId
                }
                onClick={onCellClick}
                onPointerDown={onCellPointerDown}
                onOpen={onOpen}
              />
            </div>
          ))}
        </div>
      </div>
    </div>
  );
}

/**
 * Vignette de la filmstrip, avec son marqueur, sa note et sa couleur. Un
 * bouton enfoncé si l'élément est sélectionné (les « options » sont celles
 * de la grille).
 */
const FilmstripCell = memo(function FilmstripCell({
  item,
  index,
  isSelected,
  isActive,
  onClick,
  onPointerDown,
  onOpen,
}: {
  item: GridItem | undefined;
  index: number;
  isSelected: boolean;
  isActive: boolean;
  onClick: (mediaId: number, index: number, event: MouseEvent) => void;
  /** Début possible d'un glisser de photos. */
  onPointerDown: (mediaId: number, event: PointerEvent) => void;
  onOpen: (mediaId: number) => void;
}) {
  if (item === undefined) {
    return <div className={styles.cell} aria-hidden />;
  }
  const classes = [
    styles.cell,
    isSelected ? styles.selected : undefined,
    isActive ? styles.active : undefined,
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <button
      type="button"
      tabIndex={-1}
      className={classes}
      aria-pressed={isSelected}
      aria-current={isActive ? "true" : undefined}
      aria-label={item.fileName}
      title={`${item.fileName}\n${formatCaptureTime(item.captureTime)}`}
      onClick={(event) => {
        onClick(item.id, index, event);
      }}
      onPointerDown={(event) => {
        onPointerDown(item.id, event);
      }}
      onDoubleClick={() => {
        onOpen(item.id);
      }}
    >
      <span className={gridStyles.frame}>
        <Thumbnail item={item} />
      </span>
      {item.stack !== null && <StackIndicator stack={item.stack} />}
      {item.missing && (
        <span className={gridStyles.missing} aria-hidden>
          !
        </span>
      )}
      <span className={styles.triage}>
        <TriageBadges
          flag={item.flag}
          rating={item.rating}
          colorLabel={item.colorLabel}
        />
      </span>
    </button>
  );
});
