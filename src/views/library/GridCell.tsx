import { memo } from "react";
import type { MouseEvent, PointerEvent } from "react";
import { useTranslation } from "react-i18next";

import { dropTargetAttributes } from "../../components/DragProvider";
import { counted, formatCaptureTime, formatDuration } from "../../i18n/format";
import { thumbnailUrl } from "../../services/library";
import type { GridItem, GridXmpState } from "../../services/library";
import type { GridStack } from "../../services/stacks";
import styles from "./GridView.module.css";
import { isQuarterTurn, rotationTransform } from "./rotation";
import { TriageBadges } from "./TriageBadges";

/** Signe de l'état du fichier XMP. */
const XMP_GLYPHS: Record<GridXmpState, string> = {
  pending: "↑",
  changedElsewhere: "↓",
  failed: "!",
};

const XMP_LABELS = {
  pending: "library.xmpPending",
  changedElsewhere: "library.xmpChangedElsewhere",
  failed: "library.xmpFailed",
} as const satisfies Record<GridXmpState, string>;

function xmpClasses(item: GridItem): string {
  return [
    styles.xmp,
    item.xmp === "failed" ? styles.xmpFailed : undefined,
    item.isVideo ? styles.aboveDuration : undefined,
  ]
    .filter(Boolean)
    .join(" ");
}

/**
 * Case de la grille : la vignette (ou sa place tant qu'elle n'est pas
 * prête), avec la durée pour une vidéo. Une case dont l'élément n'est pas
 * encore chargé reste vide. La case peut être glissée (vers un mot-clé) et
 * recevoir un mot-clé glissé depuis la liste.
 */
export const GridCell = memo(function GridCell({
  item,
  index,
  size,
  isSelected,
  isActive,
  isDropTarget,
  onClick,
  onPointerDown,
  onOpen,
  onToggleTarget,
  onToggleStack,
  onContextMenu,
  onXmpConflict,
}: {
  item: GridItem | undefined;
  /** Position dans la grille. */
  index: number;
  size: number;
  isSelected: boolean;
  /** Élément actif de la sélection (mis en évidence plus clairement). */
  isActive: boolean;
  /** Un mot-clé glissé sera posé ici si on le lâche. */
  isDropTarget: boolean;
  onClick: (mediaId: number, index: number, event: MouseEvent) => void;
  /** Début possible d'un glisser de photos. */
  onPointerDown: (mediaId: number, event: PointerEvent) => void;
  /** Ouvre l'élément en Loupe (double-clic). */
  onOpen: (mediaId: number) => void;
  /**
   * Ajoute l'élément à la collection cible, ou l'en retire (rond affiché au
   * survol, plein s'il y est) ; absent, pas de rond.
   */
  onToggleTarget?: (mediaId: number) => void;
  /** Déplie ou replie la pile de l'élément (clic sur son indicateur). */
  onToggleStack?: (mediaId: number) => void;
  /** Ouvre le menu contextuel de l'élément (clic droit). */
  onContextMenu?: (mediaId: number) => void;
  /**
   * Propose de lire ou d'écraser le XMP de l'élément, modifié par un autre
   * logiciel (clic sur son icône).
   */
  onXmpConflict?: (mediaId: number) => void;
}) {
  const { t } = useTranslation();
  const dimensions = { width: size, height: size };

  if (item === undefined) {
    return <div className={styles.cell} style={dimensions} aria-hidden />;
  }

  const classes = [
    styles.cell,
    isSelected ? styles.selected : undefined,
    isActive ? styles.active : undefined,
    isDropTarget ? styles.dropTarget : undefined,
  ]
    .filter(Boolean)
    .join(" ");
  return (
    <div
      {...dropTargetAttributes({ kind: "media", mediaId: item.id })}
      role="option"
      aria-selected={isSelected}
      aria-label={item.fileName}
      className={classes}
      style={dimensions}
      title={`${item.fileName}\n${formatCaptureTime(item.captureTime)}`}
      aria-current={isActive ? "true" : undefined}
      onClick={(event) => {
        onClick(item.id, index, event);
      }}
      onPointerDown={(event) => {
        onPointerDown(item.id, event);
      }}
      onDoubleClick={() => {
        onOpen(item.id);
      }}
      onContextMenu={
        onContextMenu === undefined
          ? undefined
          : (event) => {
              event.preventDefault();
              onContextMenu(item.id);
            }
      }
    >
      <div className={styles.frame}>
        <Thumbnail item={item} />
      </div>
      <span className={styles.triage}>
        <TriageBadges
          flag={item.flag}
          rating={item.rating}
          colorLabel={item.colorLabel}
        />
      </span>
      {item.xmp !== null &&
        (item.xmp === "changedElsewhere" && onXmpConflict !== undefined ? (
          <button
            type="button"
            tabIndex={-1}
            className={xmpClasses(item)}
            aria-label={t(XMP_LABELS[item.xmp])}
            title={t(XMP_LABELS[item.xmp])}
            onPointerDown={(event) => {
              // Ni glisser ni sélection : l'icône n'agit que sur son choix.
              event.stopPropagation();
            }}
            onClick={(event) => {
              event.stopPropagation();
              onXmpConflict(item.id);
            }}
          >
            {XMP_GLYPHS[item.xmp]}
          </button>
        ) : (
          <span
            className={xmpClasses(item)}
            role="img"
            aria-label={t(XMP_LABELS[item.xmp])}
            title={t(XMP_LABELS[item.xmp])}
          >
            {XMP_GLYPHS[item.xmp]}
          </span>
        ))}
      {item.missing && (
        <span
          className={styles.missing}
          role="img"
          aria-label={t("library.missingBadge")}
          title={t("library.missingBadge")}
        >
          !
        </span>
      )}
      {onToggleTarget !== undefined && (
        <button
          type="button"
          tabIndex={-1}
          className={[
            styles.target,
            item.inTarget ? styles.inTarget : undefined,
            item.missing ? styles.belowMissing : undefined,
          ]
            .filter(Boolean)
            .join(" ")}
          aria-pressed={item.inTarget}
          aria-label={t("library.targetCircle")}
          title={t("library.targetCircle")}
          onPointerDown={(event) => {
            // Ni glisser ni sélection : le rond n'agit que sur cette photo.
            event.stopPropagation();
          }}
          onClick={(event) => {
            event.stopPropagation();
            onToggleTarget(item.id);
          }}
          onDoubleClick={(event) => {
            event.stopPropagation();
          }}
        />
      )}
      {item.stack !== null && (
        <StackIndicator
          stack={item.stack}
          onToggle={
            onToggleStack === undefined
              ? undefined
              : () => {
                  onToggleStack(item.id);
                }
          }
        />
      )}
      {item.isVideo && item.durationMs !== null && (
        <span
          className={styles.badge}
          aria-label={t("library.video", {
            duration: formatDuration(item.durationMs),
          })}
        >
          {formatDuration(item.durationMs)}
        </span>
      )}
    </div>
  );
});

/**
 * Indicateur de pile, en haut à gauche : le nombre de photos d'une pile
 * repliée, ou le rang de la photo dans une pile dépliée (« 2/3 »). Un clic
 * la déplie ou la replie ; sans `onToggle`, simple étiquette.
 */
export function StackIndicator({
  stack,
  onToggle,
}: {
  stack: GridStack;
  onToggle?: () => void;
}) {
  const { t } = useTranslation();
  const text = stack.collapsed
    ? String(stack.size)
    : `${String(stack.position)}/${String(stack.size)}`;
  const label = stack.collapsed
    ? t("library.stackCollapsed", counted(stack.size))
    : t("library.stackExpanded", {
        position: stack.position,
        size: stack.size,
      });
  const className = [
    styles.stack,
    stack.collapsed ? styles.collapsedStack : undefined,
  ]
    .filter(Boolean)
    .join(" ");
  if (onToggle === undefined) {
    return (
      <span className={className} title={label}>
        {text}
      </span>
    );
  }
  return (
    <button
      type="button"
      tabIndex={-1}
      className={className}
      aria-expanded={!stack.collapsed}
      aria-label={label}
      title={label}
      onPointerDown={(event) => {
        // Ni glisser ni sélection : l'indicateur n'agit que sur la pile.
        event.stopPropagation();
      }}
      onClick={(event) => {
        event.stopPropagation();
        onToggle();
      }}
      onDoubleClick={(event) => {
        event.stopPropagation();
      }}
    >
      {text}
    </button>
  );
}

/** Vignette d'un élément, ou sa place tant qu'elle n'est pas prête. */
export function Thumbnail({ item }: { item: GridItem }) {
  const { t } = useTranslation();
  switch (item.thumbnail) {
    case "ready":
      return (
        <img
          className={
            isQuarterTurn(item.rotation)
              ? [styles.image, styles.turned].join(" ")
              : styles.image
          }
          style={{ transform: rotationTransform(item.rotation) }}
          src={thumbnailUrl(item.id, item.imageVersion)}
          // Le nom du fichier est déjà annoncé par la case elle-même.
          alt=""
          draggable={false}
          decoding="async"
        />
      );
    case "failed":
      return (
        <span className={styles.failed}>{t("library.thumbnailFailed")}</span>
      );
    case "pending":
      return <span className={styles.placeholder} />;
  }
}
