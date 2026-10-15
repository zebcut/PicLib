import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import type { GridItems } from "../../state/useGridItems";
import { useIdleCursor } from "../../state/useIdleCursor";
import { useWindowFullscreen } from "../../state/useWindowFullscreen";
import styles from "./CullView.module.css";
import { EdgeNotice, useEdgeNotice } from "./EdgeNotice";
import { positionFrom, preloadPositions } from "./photoSequence";
import type { EditActions } from "./LibraryBrowser";
import { LoupeImage } from "./LoupeImage";
import { usePreloadedPreviews } from "./usePreloadedPreviews";
import { usePhotoSequence } from "./usePhotoSequence";
import { useTriageShortcuts } from "./useTriageShortcuts";
import { FIT, fullCentered } from "./zoom";
import type { Zoom } from "./zoom";

/** Immobilité de la souris, en millisecondes, avant de masquer le pointeur. */
const CURSOR_HIDE_DELAY_MS = 2000;

/** Actions de sélection sans effet en mode Tri, qui se pilote au clavier. */
const IGNORED_SHORTCUTS = [
  "selection.above",
  "selection.below",
  "selection.extendPrevious",
  "selection.extendNext",
  "selection.extendAbove",
  "selection.extendBelow",
  "selection.all",
  "selection.none",
] as const;

/**
 * Mode Tri plein écran : la photo active seule, sur fond noir,
 * la fenêtre en plein écran. Les flèches parcourent les photos
 * sélectionnées s'il y en a plusieurs, sinon tout l'ensemble affiché ; les
 * touches de tri s'appliquent à la photo affichée seulement, avec l'avance
 * automatique propre à ce mode.
 *
 * @param onShow affiche une autre photo du parcours.
 * @param onLeave quitte le mode Tri (plein écran quitté par l'utilisateur).
 */
export function CullView({
  grid,
  selectedIds,
  activeId,
  refreshKey,
  triage,
  onShow,
  onLeave,
}: {
  grid: GridItems;
  selectedIds: ReadonlySet<number>;
  activeId: number;
  refreshKey: number;
  triage: EditActions;
  onShow: (mediaId: number) => void;
  onLeave: () => void;
}) {
  const { t } = useTranslation();
  const sequence = usePhotoSequence(grid, selectedIds, refreshKey);
  const index = grid.indexOf(activeId);
  const item = index === undefined ? undefined : grid.itemAt(index);
  const isIdle = useIdleCursor(CURSOR_HIDE_DELAY_MS);
  const edge = useEdgeNotice();
  useWindowFullscreen(onLeave);
  useCullModeMarker();

  // Le zoom est propre à une photo : en changer revient à l'affichage adapté.
  const [zoomState, setZoomState] = useState<{ mediaId: number; zoom: Zoom }>({
    mediaId: activeId,
    zoom: FIT,
  });
  const zoom = zoomState.mediaId === activeId ? zoomState.zoom : FIT;
  const setZoom = (next: Zoom) => {
    setZoomState({ mediaId: activeId, zoom: next });
  };

  const { ensureRange, itemAt, version } = grid;
  const neighbors =
    index === undefined || sequence === undefined
      ? []
      : preloadPositions(sequence, index);
  const neighborsKey = neighbors.join(",");
  useEffect(() => {
    // Positions éparses quand seule la sélection est parcourue : chacune
    // est chargée séparément.
    for (const position of neighborsKey.split(",")) {
      if (position !== "") ensureRange(Number(position), Number(position));
    }
  }, [ensureRange, neighborsKey, version]);
  usePreloadedPreviews(
    neighbors
      .map((position) => itemAt(position))
      .filter((item) => item !== undefined),
  );

  /** Passe à la photo située `delta` rangs plus loin dans le parcours. */
  const move = (delta: number) => {
    if (index === undefined || sequence === undefined) return;
    const target = positionFrom(sequence, index, delta);
    if (target === undefined) {
      edge.show(delta > 0 ? "end" : "start");
      return;
    }
    const next = itemAt(target);
    // Pas encore chargée (rare, grâce au préchargement) : l'appui est perdu.
    if (next === undefined) ensureRange(target, target);
    else onShow(next.id);
  };

  useTriageShortcuts({
    mediaIds: () => [activeId],
    activeColor: item?.colorLabel,
    apply: triage.apply,
    rotate: triage.rotate,
    onApplied: () => {
      if (triage.cullAutoAdvance) move(1);
    },
  });
  const ignore = () => undefined;
  useShortcuts({
    ...Object.fromEntries(IGNORED_SHORTCUTS.map((id) => [id, ignore])),
    "selection.previous": () => {
      move(-1);
    },
    "selection.next": () => {
      move(1);
    },
    "loupe.toggleZoom": () => {
      setZoom(zoom.mode === "fit" ? fullCentered() : FIT);
    },
  });

  return (
    <section
      className={isIdle ? [styles.cull, styles.idle].join(" ") : styles.cull}
      aria-label={t("cull.label")}
    >
      {item !== undefined && (
        <LoupeImage
          key={item.id}
          item={item}
          zoom={zoom}
          onZoomChange={setZoom}
        />
      )}
      <EdgeNotice edge={edge.current} />
    </section>
  );
}

/**
 * Signale le mode Tri sur la racine du document, le temps de son
 * affichage : la confirmation des modifications, affichée par l'application
 * au-dessus de la photo, s'agrandit alors.
 */
function useCullModeMarker(): void {
  useEffect(() => {
    const root = document.documentElement;
    root.toggleAttribute("data-cull-mode", true);
    return () => {
      root.removeAttribute("data-cull-mode");
    };
  }, []);
}
