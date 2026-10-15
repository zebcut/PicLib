import { useTranslation } from "react-i18next";

import { counted } from "../../i18n/format";
import type { JobUpdateStatus } from "../../services/jobs";
import { Button } from "../Button";
import { Modal } from "../Modal";
import styles from "./JobDialogs.module.css";
import { jobFailureMessage } from "./Lines";

/** Bilan d'une mise à la corbeille, ou explication de son interruption. */
export function TrashResult({
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
      if (status.result.kind !== "trash") return null;
      const { trashed, missing, failed } = status.result.summary;
      return (
        <Modal title={t("trashResult.title")} actions={okButton}>
          <ul className={styles.figures}>
            <li>{t("trashResult.trashed", counted(trashed))}</li>
            {missing > 0 && (
              <li className={styles.warning}>
                {t("trashResult.missing", counted(missing))}
              </li>
            )}
            {failed > 0 && (
              <li className={styles.warning}>
                {t("trashResult.failed", counted(failed))}
              </li>
            )}
          </ul>
          {trashed > 0 && (
            <p className={styles.secondary}>{t("trashResult.recover")}</p>
          )}
        </Modal>
      );
    }
    case "failed":
      return (
        <Modal title={t("trashResult.failedTitle")} actions={okButton}>
          <p className={styles.text}>{jobFailureMessage(t, status.failure)}</p>
        </Modal>
      );
    case "crashed":
      return (
        <Modal title={t("trashResult.failedTitle")} actions={okButton}>
          <p className={styles.text}>{t("trashResult.crashed")}</p>
        </Modal>
      );
  }
}
