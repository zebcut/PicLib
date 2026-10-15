import { memo, useCallback, useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { counted } from "../../i18n/format";
import { previewUrl, thumbnailUrl } from "../../services/library";
import type { GridItem } from "../../services/library";
import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import { useElementSize } from "../../state/useElementSize";
import type { GridItems } from "../../state/useGridItems";
import gridStyles from "./GridView.module.css";
import type { EditActions } from "./LibraryBrowser";
import libraryStyles from "./LibraryView.module.css";
import { isQuarterTurn, rotationTransform } from "./rotation";
import {
  activeAfterRemoval,
  needsPreview,
  neighborIndex,
  positionRanges,
  surveyLayout,
} from "./survey";
import type { SurveyMove } from "./survey";
import styles from "./SurveyView.module.css";
import { TriageBadges } from "./TriageBadges";
import { usePhotoSequence } from "./usePhotoSequence";
import { useTriageShortcuts } from "./useTriageShortcuts";

/**
 * Largeur de case, en pixels, en dessous de laquelle marqueur, étoiles et
 * couleur ne tiennent plus : ils sont masqués (la barre du bas montre ceux
 * de la photo active).
 */
const MIN_WIDTH_FOR_BADGES = 96;

/**
 * Vue Ensemble : toutes les photos sélectionnées à la fois, aussi
 * grandes que la fenêtre le permet. La photo active, encadrée, reçoit les
 * touches de tri ; un clic ou les flèches la changent. Le bouton « × » d'une
 * photo la retire de la vue, sans la supprimer ni la désélectionner.
 *
 * @param onActivate rend active une photo de la sélection.
 */
export function SurveyView({
  grid,
  selectedIds,
  activeId,
  refreshKey,
  triage,
  onActivate,
}: {
  grid: GridItems;
  selectedIds: ReadonlySet<number>;
  activeId: number;
  refreshKey: number;
  triage: EditActions;
  onActivate: (mediaId: number) => void;
}) {
  const { t } = useTranslation();
  const { ensureRange, indexOf, itemAt, version } = grid;
  const activePosition = indexOf(activeId);
  const sequence = usePhotoSequence(grid, selectedIds, refreshKey);
  // Positions des photos sélectionnées, dans l'ordre de la grille. Une seule
  // photo sélectionnée est montrée seule (la suite parcourrait sinon tout
  // l'ensemble affiché).
  const positions = useMemo(() => {
    if (selectedIds.size <= 1) {
      return activePosition === undefined ? [] : [activePosition];
    }
    if (sequence === undefined) return [];
    return Array.from({ length: sequence.length }, (_, step) =>
      sequence.positionAt(step),
    ).filter((position) => position !== undefined);
  }, [selectedIds.size, activePosition, sequence]);

  useEffect(() => {
    for (const [first, last] of positionRanges(positions)) {
      ensureRange(first, last);
    }
  }, [ensureRange, positions, version]);

  const [removed, setRemoved] = useState<ReadonlySet<number>>(() => new Set());
  const shown = positions
    .map((position) => itemAt(position))
    .filter(
      (item): item is GridItem => item !== undefined && !removed.has(item.id),
    );
  const activeIndex = shown.findIndex((item) => item.id === activeId);
  const activeItem = activeIndex === -1 ? undefined : shown[activeIndex];

  const stageRef = useRef<HTMLDivElement>(null);
  const { width, height } = useElementSize(stageRef);
  const count = shown.length;
  const layout = useMemo(
    () => surveyLayout(count, width, height),
    [count, width, height],
  );
  const usePreview = needsPreview(layout, window.devicePixelRatio);

  /** Retire une photo de la vue ; la voisine devient active si besoin. */
  const remove = (mediaId: number) => {
    if (mediaId === activeId) {
      const next = activeAfterRemoval(
        shown.map((item) => item.id),
        mediaId,
      );
      if (next !== null) onActivate(next);
    }
    setRemoved((previous) => new Set(previous).add(mediaId));
  };
  // Fonction stable pour les cases, qui ne se redessinent ainsi pas toutes
  // à chaque changement (des milliers de photos, sans limite).
  const latestRemove = useRef(remove);
  useEffect(() => {
    latestRemove.current = remove;
  });
  const stableRemove = useCallback((mediaId: number) => {
    latestRemove.current(mediaId);
  }, []);

  const move = (direction: SurveyMove) => {
    if (activeIndex === -1) return;
    const target = neighborIndex(activeIndex, count, layout.columns, direction);
    const next = target === undefined ? undefined : shown[target];
    if (next !== undefined) onActivate(next.id);
  };

  useTriageShortcuts({
    mediaIds: () => (activeItem === undefined ? [] : [activeItem.id]),
    activeColor: activeItem?.colorLabel,
    apply: triage.apply,
    rotate: triage.rotate,
    // Pas d'avance automatique dans l'Ensemble : la SPEC ne la prévoit pas.
    onApplied: () => undefined,
  });
  useShortcuts({
    "selection.previous": () => {
      move("previous");
    },
    "selection.next": () => {
      move("next");
    },
    "selection.above": () => {
      move("above");
    },
    "selection.below": () => {
      move("below");
    },
  });

  return (
    <>
      <div ref={stageRef} className={styles.survey}>
        {count === 0 && removed.size > 0 ? (
          <p className={styles.empty}>{t("survey.allRemoved")}</p>
        ) : (
          <div
            role="listbox"
            aria-label={t("survey.label")}
            className={styles.grid}
            style={{
              gridTemplateColumns: `repeat(${String(layout.columns)}, ${String(layout.cellWidth)}px)`,
              gridAutoRows: `${String(layout.cellHeight)}px`,
              gap: `${String(layout.gap)}px`,
              padding: `${String(layout.gap)}px`,
            }}
          >
            {shown.map((item) => (
              <SurveyCell
                key={item.id}
                item={item}
                isActive={item.id === activeId}
                usePreview={usePreview}
                showBadges={layout.cellWidth >= MIN_WIDTH_FOR_BADGES}
                onActivate={onActivate}
                onRemove={stableRemove}
              />
            ))}
          </div>
        )}
      </div>
      <footer className={libraryStyles.toolbar}>
        <span className={libraryStyles.itemInfo}>
          {activeItem !== undefined && (
            <>
              <span className={libraryStyles.fileName}>
                {activeItem.fileName}
              </span>
              <TriageBadges
                flag={activeItem.flag}
                rating={activeItem.rating}
                colorLabel={activeItem.colorLabel}
              />
            </>
          )}
        </span>
        <span>{t("survey.count", counted(count))}</span>
      </footer>
    </>
  );
}

/** Une photo de l'Ensemble, avec son bouton pour la retirer de la vue. */
const SurveyCell = memo(function SurveyCell({
  item,
  isActive,
  usePreview,
  showBadges,
  onActivate,
  onRemove,
}: {
  item: GridItem;
  isActive: boolean;
  usePreview: boolean;
  showBadges: boolean;
  onActivate: (mediaId: number) => void;
  onRemove: (mediaId: number) => void;
}) {
  const { t } = useTranslation();
  const label = t("survey.remove", { name: item.fileName });
  return (
    <div
      role="option"
      aria-selected={isActive}
      aria-current={isActive ? "true" : undefined}
      aria-label={item.fileName}
      title={item.fileName}
      className={
        isActive ? [styles.cell, styles.active].join(" ") : styles.cell
      }
      onClick={() => {
        onActivate(item.id);
      }}
    >
      <div className={gridStyles.frame}>
        <SurveyImage item={item} usePreview={usePreview} />
      </div>
      {showBadges && (
        <span className={gridStyles.triage}>
          <TriageBadges
            flag={item.flag}
            rating={item.rating}
            colorLabel={item.colorLabel}
          />
        </span>
      )}
      <button
        type="button"
        className={styles.remove}
        aria-label={label}
        title={label}
        onClick={(event) => {
          // Retirer la photo ne doit pas aussi la rendre active.
          event.stopPropagation();
          onRemove(item.id);
        }}
      >
        ×
      </button>
    </div>
  );
});

/**
 * Image d'une photo : la vignette, ou l'aperçu de la Loupe pour une grande
 * case (la vignette reste affichée pendant son chargement, ou s'il échoue).
 */
function SurveyImage({
  item,
  usePreview,
}: {
  item: GridItem;
  usePreview: boolean;
}) {
  const { t } = useTranslation();
  const [isPreviewReady, setIsPreviewReady] = useState(false);
  const imageClass = isQuarterTurn(item.rotation)
    ? [gridStyles.image, gridStyles.turned].join(" ")
    : gridStyles.image;
  const style = { transform: rotationTransform(item.rotation) };
  const showThumbnail = !usePreview || !isPreviewReady;

  return (
    <>
      {showThumbnail &&
        (item.thumbnail === "ready" ? (
          <img
            className={imageClass}
            style={style}
            src={thumbnailUrl(item.id, item.imageVersion)}
            // Le nom du fichier est déjà annoncé par la case elle-même.
            alt=""
            draggable={false}
            decoding="async"
          />
        ) : item.thumbnail === "failed" ? (
          <span className={gridStyles.failed}>
            {t("library.thumbnailFailed")}
          </span>
        ) : (
          <span className={gridStyles.placeholder} />
        ))}
      {usePreview && (
        <img
          className={isPreviewReady ? imageClass : styles.loading}
          style={style}
          src={previewUrl(item.id, item.imageVersion)}
          alt=""
          draggable={false}
          decoding="async"
          onLoad={() => {
            setIsPreviewReady(true);
          }}
        />
      )}
    </>
  );
}
