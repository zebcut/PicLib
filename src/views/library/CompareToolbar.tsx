import { useTranslation } from "react-i18next";

import type { GridItem } from "../../services/library";
import compareStyles from "./CompareView.module.css";
import styles from "./LibraryView.module.css";
import { TriageBadges } from "./TriageBadges";
import type { Zoom } from "./zoom";

/**
 * Barre sous la Comparaison : nom, marqueur, note et couleur de la sélection
 * et de la candidate, les boutons Échanger et Choisir, et le choix entre
 * l'affichage adapté et 100 %.
 */
export function CompareToolbar({
  select,
  candidate,
  zoom,
  onSwap,
  onChoose,
  onToggleZoom,
}: {
  select: GridItem | undefined;
  candidate: GridItem | undefined;
  zoom: Zoom;
  onSwap: () => void;
  onChoose: () => void;
  onToggleZoom: () => void;
}) {
  const { t } = useTranslation();
  const hasCandidate = candidate !== undefined;

  return (
    <footer className={styles.toolbar}>
      <ItemSummary label={t("compare.select")} item={select} />
      <span className={compareStyles.actions}>
        <button
          type="button"
          className={styles.zoomButton}
          title={t("compare.swapTooltip")}
          disabled={!hasCandidate}
          onClick={onSwap}
        >
          {t("compare.swap")}
        </button>
        <button
          type="button"
          className={styles.zoomButton}
          title={t("compare.chooseTooltip")}
          disabled={!hasCandidate}
          onClick={onChoose}
        >
          {t("compare.choose")}
        </button>
        <button
          type="button"
          className={styles.zoomButton}
          aria-pressed={zoom.mode === "full"}
          title={t("loupe.zoomTooltip")}
          onClick={onToggleZoom}
        >
          {zoom.mode === "full" ? t("loupe.zoomFull") : t("loupe.zoomFit")}
        </button>
      </span>
      <ItemSummary label={t("compare.candidate")} item={candidate} />
    </footer>
  );
}

/** Nom et valeurs de tri d'une des deux photos. */
function ItemSummary({
  label,
  item,
}: {
  label: string;
  item: GridItem | undefined;
}) {
  return (
    <span className={styles.itemInfo}>
      <span>{label}</span>
      {item !== undefined && (
        <>
          <span className={styles.fileName}>{item.fileName}</span>
          <TriageBadges
            flag={item.flag}
            rating={item.rating}
            colorLabel={item.colorLabel}
          />
        </>
      )}
    </span>
  );
}
