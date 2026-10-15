import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import type { GridItem } from "../../services/library";
import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import type { GridItems } from "../../state/useGridItems";
import { initialCandidate, nextCandidate } from "./compare";
import { CompareToolbar } from "./CompareToolbar";
import styles from "./CompareView.module.css";
import { EdgeNotice, useEdgeNotice } from "./EdgeNotice";
import type { EditActions } from "./LibraryBrowser";
import { LoupeImage } from "./LoupeImage";
import { PanLink } from "./panLink";
import { usePhotoSequence } from "./usePhotoSequence";
import { usePreloadedPreviews } from "./usePreloadedPreviews";
import { useTriageShortcuts } from "./useTriageShortcuts";
import { FIT, fullCentered } from "./zoom";
import type { Zoom } from "./zoom";

/** Les deux côtés de la Comparaison. */
type Side = "select" | "candidate";

/**
 * Vue Comparaison : la photo active, la « sélection », à côté
 * d'une « candidate » prise parmi les photos parcourues (la sélection de la
 * bibliothèque si elle compte plusieurs photos, sinon tout l'ensemble
 * affiché). Les flèches changent de candidate ; ↑ la choisit, ↓ échange les
 * deux. Zoom et déplacement sont communs aux deux photos. Les touches de tri
 * s'appliquent à la photo qui a la main (la candidate, au départ).
 *
 * @param onShow rend une autre photo active : elle devient la sélection.
 */
export function CompareView({
  grid,
  selectedIds,
  activeId,
  refreshKey,
  triage,
  onShow,
}: {
  grid: GridItems;
  selectedIds: ReadonlySet<number>;
  activeId: number;
  refreshKey: number;
  triage: EditActions;
  onShow: (mediaId: number) => void;
}) {
  const { t } = useTranslation();
  const sequence = usePhotoSequence(grid, selectedIds, refreshKey);
  const { ensureRange, indexOf, itemAt, version } = grid;
  const selectPosition = indexOf(activeId);
  const select =
    selectPosition === undefined ? undefined : itemAt(selectPosition);

  // Candidate choisie aux flèches pour cette sélection ; sinon, la voisine.
  const [chosen, setChosen] = useState<{
    selectId: number;
    candidateId: number;
  } | null>(null);
  const candidatePosition =
    chosen !== null && chosen.selectId === activeId
      ? indexOf(chosen.candidateId)
      : selectPosition === undefined || sequence === undefined
        ? undefined
        : initialCandidate(sequence, selectPosition);
  const candidate =
    candidatePosition === undefined ? undefined : itemAt(candidatePosition);

  const [focus, setFocus] = useState<Side>("candidate");
  const focusedSide: Side =
    focus === "candidate" && candidate !== undefined ? "candidate" : "select";
  const target = focusedSide === "candidate" ? candidate : select;

  const [zoom, setZoomState] = useState<Zoom>(FIT);
  const [panLink] = useState(() => new PanLink());
  const setZoom = (next: Zoom) => {
    // Un nouveau zoom repart du point cliqué, pour les deux photos.
    panLink.reset();
    setZoomState(next);
  };
  const edge = useEdgeNotice();

  const neighbors =
    sequence === undefined ||
    selectPosition === undefined ||
    candidatePosition === undefined
      ? []
      : [1, -1]
          .map((direction) =>
            nextCandidate(
              sequence,
              selectPosition,
              candidatePosition,
              direction as 1 | -1,
            ),
          )
          .filter((position) => position !== undefined);
  const toLoad = candidatePosition === undefined ? [] : [candidatePosition];
  const positionsKey = [...toLoad, ...neighbors].join(",");
  useEffect(() => {
    // Positions éparses quand seule la sélection est parcourue : chacune
    // est chargée séparément.
    for (const position of positionsKey.split(",")) {
      if (position !== "") ensureRange(Number(position), Number(position));
    }
  }, [ensureRange, positionsKey, version]);
  usePreloadedPreviews(
    neighbors
      .map((position) => itemAt(position))
      .filter((item) => item !== undefined),
  );

  /** Passe à la candidate voisine, dans le sens `direction`. */
  const moveCandidate = (direction: 1 | -1) => {
    if (
      sequence === undefined ||
      selectPosition === undefined ||
      candidatePosition === undefined
    ) {
      return;
    }
    const position = nextCandidate(
      sequence,
      selectPosition,
      candidatePosition,
      direction,
    );
    if (position === undefined) {
      edge.show(direction > 0 ? "end" : "start");
      return;
    }
    const next = itemAt(position);
    // Pas encore chargée (rare, grâce au préchargement) : l'appui est perdu.
    if (next === undefined) ensureRange(position, position);
    else setChosen({ selectId: activeId, candidateId: next.id });
  };

  /** Sélection et candidate changent de côté. */
  const swap = () => {
    if (candidate === undefined) return;
    setChosen({ selectId: candidate.id, candidateId: activeId });
    onShow(candidate.id);
  };

  /** La candidate devient la sélection ; la suivante devient candidate. */
  const choose = () => {
    if (candidate === undefined) return;
    setChosen(null);
    onShow(candidate.id);
  };

  const toggleZoom = () => {
    setZoom(zoom.mode === "fit" ? fullCentered() : FIT);
  };

  useTriageShortcuts({
    mediaIds: () => (target === undefined ? [] : [target.id]),
    activeColor: target?.colorLabel,
    apply: triage.apply,
    rotate: triage.rotate,
    // Pas d'avance automatique en Comparaison : la SPEC ne la prévoit pas.
    onApplied: () => undefined,
  });
  useShortcuts({
    "selection.previous": () => {
      moveCandidate(-1);
    },
    "selection.next": () => {
      moveCandidate(1);
    },
    "compare.choose": choose,
    "compare.swap": swap,
    "loupe.toggleZoom": toggleZoom,
  });

  const pane = (side: Side, item: GridItem | undefined) => (
    <ComparePane
      label={t(`compare.${side}`)}
      item={item}
      isFocused={focusedSide === side}
      emptyMessage={
        side === "candidate" && sequence !== undefined
          ? t("compare.noCandidate")
          : null
      }
      zoom={zoom}
      panLink={panLink}
      onFocus={() => {
        setFocus(side);
      }}
      onZoomChange={setZoom}
    />
  );

  return (
    <>
      <section className={styles.compare} aria-label={t("compare.label")}>
        {pane("select", select)}
        {pane("candidate", candidate)}
        <EdgeNotice edge={edge.current} />
      </section>
      <CompareToolbar
        select={select}
        candidate={candidate}
        zoom={zoom}
        onSwap={swap}
        onChoose={choose}
        onToggleZoom={toggleZoom}
      />
    </>
  );
}

/**
 * Un côté de la Comparaison : son titre et sa photo. Un clic lui donne la
 * main ; sur la photo, il zoome aussi les deux côtés.
 */
function ComparePane({
  label,
  item,
  isFocused,
  emptyMessage,
  zoom,
  panLink,
  onFocus,
  onZoomChange,
}: {
  label: string;
  item: GridItem | undefined;
  isFocused: boolean;
  emptyMessage: string | null;
  zoom: Zoom;
  panLink: PanLink;
  onFocus: () => void;
  onZoomChange: (zoom: Zoom) => void;
}) {
  return (
    <div
      role="region"
      aria-label={label}
      aria-current={isFocused}
      className={
        isFocused ? [styles.pane, styles.focused].join(" ") : styles.pane
      }
      onPointerDown={onFocus}
    >
      <p className={styles.paneLabel}>{label}</p>
      <div className={styles.paneStage}>
        {item !== undefined ? (
          <LoupeImage
            key={item.id}
            item={item}
            zoom={zoom}
            panLink={panLink}
            onZoomChange={onZoomChange}
          />
        ) : (
          emptyMessage !== null && (
            <p className={styles.empty}>{emptyMessage}</p>
          )
        )}
      </div>
    </div>
  );
}
