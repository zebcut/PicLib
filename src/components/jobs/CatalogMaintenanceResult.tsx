import { useTranslation } from "react-i18next";

import { counted, formatBytes } from "../../i18n/format";
import type { JobUpdateStatus } from "../../services/jobs";
import { hasSomethingToClean } from "../../services/maintenance";
import { Button } from "../Button";
import { Modal } from "../Modal";
import styles from "./JobDialogs.module.css";
import { jobFailureMessage } from "./Lines";

type FinishedStatus = Exclude<
  JobUpdateStatus,
  { status: "running" | "cancelled" }
>;

/**
 * Summary of a catalog check, to confirm before cleaning up (a bulk
 * operation always shows a summary first); or a damaged catalog, which is
 * left untouched.
 *
 * @param onClean starts the cleanup, then closes.
 * @param onClose closes without changing anything.
 */
export function CatalogCheckResult({
  status,
  onClean,
  onClose,
}: {
  status: FinishedStatus;
  onClean: () => void;
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
      if (status.result.kind !== "catalogCheck") return null;
      const { summary } = status.result;
      if (summary.problems.length > 0) {
        return (
          <Modal
            title={t("catalogMaintenance.damagedTitle")}
            actions={okButton}
          >
            <p className={styles.warning}>{t("catalogMaintenance.damaged")}</p>
            <ul className={styles.paths}>
              {summary.problems.map((problem) => (
                <li key={problem}>{problem}</li>
              ))}
            </ul>
          </Modal>
        );
      }
      const orphans = summary.orphanThumbnails + summary.orphanPreviews;
      return (
        <Modal
          title={t("catalogMaintenance.title")}
          actions={
            <>
              <Button onClick={onClose}>{t("common.cancel")}</Button>
              <Button variant="primary" autoFocus onClick={onClean}>
                {t("catalogMaintenance.clean")}
              </Button>
            </>
          }
        >
          <p className={styles.text}>
            {t("catalogMaintenance.sound", {
              size: formatBytes(summary.catalogBytes),
            })}
          </p>
          {hasSomethingToClean(summary) ? (
            <ul className={styles.figures}>
              {orphans > 0 && (
                <li>
                  {t("catalogMaintenance.orphans", {
                    ...counted(orphans),
                    size: formatBytes(summary.orphanBytes),
                  })}
                </li>
              )}
              {summary.loneStacks > 0 && (
                <li>
                  {t(
                    "catalogMaintenance.loneStacks",
                    counted(summary.loneStacks),
                  )}
                </li>
              )}
              {summary.reclaimableBytes > 0 && (
                <li>
                  {t("catalogMaintenance.reclaimable", {
                    size: formatBytes(summary.reclaimableBytes),
                  })}
                </li>
              )}
            </ul>
          ) : (
            <p className={styles.text}>{t("catalogMaintenance.nothing")}</p>
          )}
          <p className={styles.secondary}>
            {t("catalogMaintenance.explanation")}
          </p>
        </Modal>
      );
    }
    case "failed":
      return (
        <Modal title={t("catalogMaintenance.failedTitle")} actions={okButton}>
          <p className={styles.text}>{jobFailureMessage(t, status.failure)}</p>
        </Modal>
      );
    case "crashed":
      return (
        <Modal title={t("catalogMaintenance.failedTitle")} actions={okButton}>
          <p className={styles.text}>{t("catalogMaintenance.crashed")}</p>
        </Modal>
      );
  }
}

/** What a cleanup did, or why it stopped. */
export function CatalogCleanupResult({
  status,
  onClose,
}: {
  status: FinishedStatus;
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
      if (status.result.kind !== "catalogCleanup") return null;
      const { summary } = status.result;
      return (
        <Modal title={t("catalogMaintenance.doneTitle")} actions={okButton}>
          <ul className={styles.figures}>
            <li>
              {t("catalogMaintenance.removedImages", {
                ...counted(summary.removedImages),
                size: formatBytes(summary.removedBytes),
              })}
            </li>
            {summary.removedStacks > 0 && (
              <li>
                {t(
                  "catalogMaintenance.removedStacks",
                  counted(summary.removedStacks),
                )}
              </li>
            )}
            <li>
              {t("catalogMaintenance.catalogSize", {
                before: formatBytes(summary.catalogBytesBefore),
                after: formatBytes(summary.catalogBytesAfter),
              })}
            </li>
          </ul>
          <p className={styles.secondary}>
            {t("catalogMaintenance.backup", { name: summary.backupName })}
          </p>
        </Modal>
      );
    }
    case "failed":
      return (
        <Modal title={t("catalogMaintenance.failedTitle")} actions={okButton}>
          <p className={styles.text}>{jobFailureMessage(t, status.failure)}</p>
        </Modal>
      );
    case "crashed":
      return (
        <Modal title={t("catalogMaintenance.failedTitle")} actions={okButton}>
          <p className={styles.text}>{t("catalogMaintenance.crashed")}</p>
        </Modal>
      );
  }
}
