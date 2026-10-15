import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { describeSummary } from "../i18n/triage";
import type { EditNotice } from "../state/useCatalogEdits";
import styles from "./EditNoticeToast.module.css";

/** Durée d'affichage d'une confirmation, en millisecondes. */
const NOTICE_DURATION_MS = 1500;

/**
 * Brève confirmation d'une modification, annulation ou rétablissement
 * (« Note : 3 étoiles »), ou message d'échec, en bas de la fenêtre.
 */
export function EditNoticeToast({ notice }: { notice: EditNotice | null }) {
  const { t } = useTranslation();
  const [visibleKey, setVisibleKey] = useState<number | null>(null);
  const key = notice?.key ?? null;

  useEffect(() => {
    if (key === null) return;
    const show = setTimeout(() => {
      setVisibleKey(key);
    }, 0);
    const hide = setTimeout(() => {
      setVisibleKey(null);
    }, NOTICE_DURATION_MS);
    return () => {
      clearTimeout(show);
      clearTimeout(hide);
    };
  }, [key]);

  if (notice === null) return null;
  if (visibleKey !== notice.key) return null;
  if (notice.kind === "refused") {
    const { refusal } = notice;
    return (
      <p className={[styles.toast, styles.error].join(" ")} role="alert">
        {refusal.area === "stacks"
          ? t(`stacks.refusal.${refusal.reason}`)
          : refusal.area === "relocation"
            ? t(`relocation.refusal.${refusal.reason}`)
            : t(`fileMove.refusal.${refusal.reason}`)}
      </p>
    );
  }
  if (notice.kind === "info") {
    return (
      <p className={styles.toast} role="status">
        {notice.message}
      </p>
    );
  }
  if (notice.kind === "failed") {
    return (
      <p className={[styles.toast, styles.error].join(" ")} role="alert">
        {t("triage.failed")}
      </p>
    );
  }
  return (
    <p className={styles.toast} role="status">
      {t(`triage.${notice.kind}`, {
        description: describeSummary(t, notice.summary),
      })}
    </p>
  );
}
