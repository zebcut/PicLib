import { useTranslation } from "react-i18next";

import { counted } from "../../i18n/format";
import type { JobUpdateStatus } from "../../services/jobs";
import { Button } from "../Button";
import { Modal } from "../Modal";
import styles from "./JobDialogs.module.css";
import { jobFailureMessage } from "./Lines";

/**
 * Bilan de « Enregistrer les métadonnées dans les fichiers »,
 * montré seulement quand des fichiers n'ont pas été écrits (voir
 * `useJobs`) ; ou explication d'une interruption.
 */
export function XmpWriteResult({
  status,
  onClose,
}: {
  status: Exclude<JobUpdateStatus, { status: "running" | "cancelled" }>;
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
      if (status.result.kind !== "xmpWrite") return null;
      const { written, changedElsewhere, failed } = status.result.summary;
      return (
        <Modal title={t("xmpWrite.title")} actions={okButton}>
          <ul className={styles.figures}>
            <li>{t("xmpWrite.written", counted(written))}</li>
            {changedElsewhere > 0 && (
              <li className={styles.warning}>
                {t("xmpWrite.changedElsewhere", counted(changedElsewhere))}
              </li>
            )}
            {failed > 0 && (
              <li className={styles.warning}>
                {t("xmpWrite.failed", counted(failed))}
              </li>
            )}
          </ul>
        </Modal>
      );
    }
    case "failed":
      return (
        <Modal title={t("xmpWrite.failedTitle")} actions={okButton}>
          <p className={styles.text}>{jobFailureMessage(t, status.failure)}</p>
        </Modal>
      );
    case "crashed":
      return (
        <Modal title={t("xmpWrite.failedTitle")} actions={okButton}>
          <p className={styles.text}>{t("xmpWrite.crashed")}</p>
        </Modal>
      );
  }
}
