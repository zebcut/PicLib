import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { counted } from "../../i18n/format";
import type { JobFailure } from "../../services/jobs";
import styles from "./JobDialogs.module.css";

/**
 * Éléments illisibles d'une analyse ou d'un import (ou non copiés), avec
 * quelques exemples.
 * N'affiche rien s'il n'y en a pas.
 */
export function UnreadableLines({
  count,
  examples,
  messageKey,
}: {
  count: number;
  examples: string[];
  messageKey:
    | "folderAnalysis.unreadable"
    | "importResult.unreadable"
    | "importResult.copyFailed"
    | "importResult.backupFailed";
}) {
  const { t } = useTranslation();
  if (count === 0) return null;

  return (
    <>
      <p className={styles.warning}>{t(messageKey, counted(count))}</p>
      <ul className={styles.paths}>
        {examples.map((path) => (
          <li key={path}>{path}</li>
        ))}
      </ul>
    </>
  );
}

/** Message traduit expliquant l'échec prévu d'une tâche. */
export function jobFailureMessage(t: TFunction, failure: JobFailure): string {
  switch (failure.kind) {
    case "cameraCard":
      return t("folderAnalysis.cameraCard");
    case "sourceUnavailable":
      return t("folderAnalysis.sourceUnavailable", { path: failure.path });
    case "notAFolder":
      return t("folderAnalysis.notAFolder", { path: failure.path });
    case "destinationFull":
      return t("importResult.destinationFull", { path: failure.path });
    case "destinationUnavailable":
      return t("importResult.destinationUnavailable", { path: failure.path });
    case "catalogError":
      return t("importResult.catalogError");
  }
}
