import { useTranslation } from "react-i18next";

import {
  formatCaptureTime,
  formatCount,
  formatDuration,
} from "../../i18n/format";
import type { GridItem } from "../../services/library";
import styles from "./LibraryView.module.css";
import { TriageBadges } from "./TriageBadges";
import type { Zoom } from "./zoom";

/**
 * Barre sous la Loupe : nom et date de l'élément, sa position, et le choix
 * entre l'affichage adapté et 100 %.
 */
export function LoupeToolbar({
  item,
  position,
  count,
  zoom,
  onToggleZoom,
}: {
  item: GridItem | undefined;
  /** Position de l'élément, à partir de 1. */
  position: number | undefined;
  count: number;
  zoom: Zoom;
  onToggleZoom: () => void;
}) {
  const { t } = useTranslation();

  return (
    <footer className={styles.toolbar}>
      <span className={styles.itemInfo}>
        {item !== undefined && (
          <>
            <span className={styles.fileName}>{item.fileName}</span>
            <TriageBadges
              flag={item.flag}
              rating={item.rating}
              colorLabel={item.colorLabel}
            />
            <span>{formatCaptureTime(item.captureTime)}</span>
            {item.isVideo && item.durationMs !== null && (
              <span>
                {t("loupe.video", {
                  duration: formatDuration(item.durationMs),
                })}
              </span>
            )}
          </>
        )}
      </span>
      {position !== undefined && (
        <span>
          {t("loupe.position", {
            position: formatCount(position),
            count: formatCount(count),
          })}
        </span>
      )}
      <button
        type="button"
        className={styles.zoomButton}
        aria-pressed={zoom.mode === "full"}
        title={t("loupe.zoomTooltip")}
        onClick={onToggleZoom}
      >
        {zoom.mode === "full" ? t("loupe.zoomFull") : t("loupe.zoomFit")}
      </button>
    </footer>
  );
}
