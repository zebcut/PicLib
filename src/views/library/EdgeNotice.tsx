import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import styles from "./EdgeNotice.module.css";

/** Durée d'affichage de « Début » ou « Fin de la liste », en millisecondes. */
const EDGE_NOTICE_DURATION_MS = 1500;

/** Extrémité de la liste atteinte. */
export type Edge = "start" | "end";

/** Message d'extrémité affiché, et de quoi en afficher un. */
export interface EdgeNoticeState {
  current: Edge | null;
  show: (edge: Edge) => void;
}

/**
 * Bref message « Début de la liste » ou « Fin de la liste », quand les
 * flèches butent sur une extrémité (mode Tri, Comparaison).
 */
export function useEdgeNotice(): EdgeNoticeState {
  const [notice, setNotice] = useState<{ edge: Edge; key: number } | null>(
    null,
  );
  const key = notice?.key ?? null;

  useEffect(() => {
    if (key === null) return;
    const timer = setTimeout(() => {
      setNotice(null);
    }, EDGE_NOTICE_DURATION_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [key]);

  return {
    current: notice?.edge ?? null,
    show: (edge) => {
      // Une nouvelle clé relance la durée d'affichage à chaque appui.
      setNotice((previous) => ({ edge, key: (previous?.key ?? 0) + 1 }));
    },
  };
}

/** Le message, en haut de la zone qui le contient (positionnée). */
export function EdgeNotice({ edge }: { edge: Edge | null }) {
  const { t } = useTranslation();
  if (edge === null) return null;
  return (
    <p className={styles.edgeNotice} role="status">
      {t(`browse.${edge}`)}
    </p>
  );
}
