import { useTranslation } from "react-i18next";

import type { GridItem } from "../../services/library";
import styles from "./TriageBadges.module.css";

/** Symboles des marqueurs (des icônes, pas du texte à traduire). */
const FLAG_SYMBOLS = { picked: "⚑", rejected: "✕" } as const;

/**
 * Marqueur, note et étiquette de couleur d'un élément, en style compact
 * ; rien si l'élément n'en a aucun.
 */
export function TriageBadges({
  flag,
  rating,
  colorLabel,
}: Pick<GridItem, "flag" | "rating" | "colorLabel">) {
  const { t } = useTranslation();

  return (
    <span className={styles.badges}>
      {flag !== null && (
        <span
          className={[styles.flag, styles[flag]].join(" ")}
          role="img"
          aria-label={t(`triage.${flag}Label`)}
          title={t(`triage.${flag}Label`)}
        >
          {FLAG_SYMBOLS[flag]}
        </span>
      )}
      {rating > 0 && (
        <span
          className={styles.stars}
          role="img"
          aria-label={t("triage.stars", { count: rating })}
          title={t("triage.stars", { count: rating })}
        >
          {"★".repeat(rating)}
        </span>
      )}
      {colorLabel !== null && (
        <span
          className={[styles.color, styles[colorLabel]].join(" ")}
          role="img"
          aria-label={t("triage.colorLabel", {
            color: t(`triage.${colorLabel}`),
          })}
          title={t("triage.colorLabel", { color: t(`triage.${colorLabel}`) })}
        />
      )}
    </span>
  );
}
