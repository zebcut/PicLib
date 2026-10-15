import { useTranslation } from "react-i18next";

import { counted } from "../../i18n/format";
import { appliesDirectly } from "../../services/fileMetadata";
import type { FileMetadataSummary } from "../../services/fileMetadata";
import type { JobUpdateStatus } from "../../services/jobs";
import { Button } from "../Button";
import { Modal } from "../Modal";
import styles from "./JobDialogs.module.css";
import { jobFailureMessage } from "./Lines";

/** Sortes d'informations complétées, dans l'ordre du résumé. */
const CATEGORIES = [
  "triage",
  "fields",
  "keywords",
  "gps",
  "captureTime",
  "details",
] as const satisfies readonly (keyof FileMetadataSummary)[];

/**
 * Bilan de « Lire les métadonnées des fichiers » : ce qui serait
 * complété, à confirmer au-delà d'une photo ; ou rien de nouveau ; ou
 * l'explication d'une interruption. Une seule photo à compléter l'est sans
 * confirmation (`useFileMetadataReading`) : rien n'est alors affiché.
 *
 * @param onApply complète les photos (annulable), puis ferme.
 * @param onClose ferme sans rien modifier.
 */
export function FileMetadataResult({
  status,
  onApply,
  onClose,
}: {
  status: Exclude<JobUpdateStatus, { status: "running" | "cancelled" }>;
  onApply: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const okButton = (
    <Button variant="primary" autoFocus onClick={onClose}>
      {t("common.ok")}
    </Button>
  );

  switch (status.status) {
    case "completed": {
      if (status.result.kind !== "fileMetadata") return null;
      const { summary } = status.result;
      if (appliesDirectly(summary)) return null;
      if (summary.toComplete === 0) {
        return (
          <Modal title={t("fileMetadata.title")} actions={okButton}>
            <p className={styles.text}>{t("fileMetadata.nothing")}</p>
            <Problems summary={summary} />
          </Modal>
        );
      }
      return (
        <Modal
          title={t("fileMetadata.title")}
          actions={
            <>
              <Button onClick={onClose}>{t("common.cancel")}</Button>
              <Button variant="primary" autoFocus onClick={onApply}>
                {t("fileMetadata.apply")}
              </Button>
            </>
          }
        >
          <p className={styles.text}>
            {t("fileMetadata.toComplete", counted(summary.toComplete))}
          </p>
          <ul className={styles.figures}>
            {CATEGORIES.filter((category) => summary[category] > 0).map(
              (category) => (
                <li key={category}>
                  {t(`fileMetadata.${category}`, counted(summary[category]))}
                </li>
              ),
            )}
          </ul>
          <Problems summary={summary} />
          <p className={styles.secondary}>{t("fileMetadata.explanation")}</p>
        </Modal>
      );
    }
    case "failed":
      return (
        <Modal title={t("fileMetadata.failedTitle")} actions={okButton}>
          <p className={styles.text}>{jobFailureMessage(t, status.failure)}</p>
        </Modal>
      );
    case "crashed":
      return (
        <Modal title={t("fileMetadata.failedTitle")} actions={okButton}>
          <p className={styles.text}>{t("fileMetadata.crashed")}</p>
        </Modal>
      );
  }
}

/** Fichiers introuvables et métadonnées illisibles ; rien s'il n'y en a pas. */
function Problems({ summary }: { summary: FileMetadataSummary }) {
  const { t } = useTranslation();
  return (
    <>
      {summary.missing > 0 && (
        <p className={styles.warning}>
          {t("fileMetadata.missing", counted(summary.missing))}
        </p>
      )}
      {summary.unreadableMetadata > 0 && (
        <p className={styles.warning}>
          {t("fileMetadata.unreadable", counted(summary.unreadableMetadata))}
        </p>
      )}
    </>
  );
}
