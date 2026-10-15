import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import type { GridItems } from "../../state/useGridItems";
import { LoupeImage } from "./LoupeImage";
import { LoupeToolbar } from "./LoupeToolbar";
import { usePreloadedPreviews } from "./usePreloadedPreviews";
import styles from "./LoupeView.module.css";
import { FIT, fullCentered } from "./zoom";
import type { Zoom } from "./zoom";

/**
 * Loupe : l'élément sélectionné en grand, adapté à la fenêtre ou à 100 %.
 * Les flèches passent à l'élément précédent ou suivant, dont l'aperçu est
 * chargé à l'avance pour un affichage instantané.
 */
export function LoupeView({
  grid,
  selectedId,
  onSelect,
}: {
  grid: GridItems;
  selectedId: number;
  onSelect: (mediaId: number) => void;
}) {
  const { t } = useTranslation();
  const index = grid.indexOf(selectedId);
  const item = index === undefined ? undefined : grid.itemAt(index);
  const count = grid.count ?? 0;
  // Le zoom est propre à une photo : en changer revient à l'affichage adapté.
  const [zoomState, setZoomState] = useState<{ mediaId: number; zoom: Zoom }>({
    mediaId: selectedId,
    zoom: FIT,
  });
  const zoom = zoomState.mediaId === selectedId ? zoomState.zoom : FIT;
  const setZoom = (next: Zoom) => {
    setZoomState({ mediaId: selectedId, zoom: next });
  };

  const { ensureRange, itemAt, version } = grid;
  useEffect(() => {
    if (index !== undefined) ensureRange(index - 1, index + 1);
  }, [ensureRange, index, version]);

  const neighbors = index === undefined ? [] : [index - 1, index + 1];
  usePreloadedPreviews(
    neighbors
      .map((neighbor) => itemAt(neighbor))
      .filter((item) => item !== undefined),
  );

  /** Passe à l'élément situé `delta` places plus loin, s'il existe. */
  const move = (delta: number) => {
    if (index === undefined) return;
    const target = itemAt(index + delta);
    if (target !== undefined) onSelect(target.id);
  };

  useShortcuts({
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
    <>
      <section className={styles.stage} aria-label={t("loupe.label")}>
        {item !== undefined && (
          <LoupeImage
            key={item.id}
            item={item}
            zoom={zoom}
            onZoomChange={setZoom}
          />
        )}
      </section>
      <LoupeToolbar
        item={item}
        position={index === undefined ? undefined : index + 1}
        count={count}
        zoom={zoom}
        onToggleZoom={() => {
          setZoom(zoom.mode === "fit" ? fullCentered() : FIT);
        }}
      />
    </>
  );
}
