import { useState } from "react";
import { useTranslation } from "react-i18next";

import { counted } from "../../i18n/format";
import type { ImportSummary } from "../../services/importFolder";
import { ejectCard } from "../../services/importRequests";
import type { JobUpdateStatus } from "../../services/jobs";
import { Button } from "../Button";
import { Modal } from "../Modal";
import styles from "./JobDialogs.module.css";
import { jobFailureMessage, UnreadableLines } from "./Lines";

/**
 * Bilan d'un import terminé, ou explication de son échec. Après la copie
 * d'une carte mémoire, il propose de l'éjecter.
 *
 * @param card nom de la carte importée, `null` si la source n'en est pas une.
 */
export function ImportResult({
  jobId,
  folderName,
  card,
  status,
  onClose,
}: {
  jobId: number;
  folderName: string;
  card: string | null;
  status: Exclude<JobUpdateStatus, { status: "running" | "cancelled" }>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [eject, setEject] = useState<
    "idle" | "ejecting" | "ejected" | "refused"
  >("idle");
  const okButton = (
    <Button variant="primary" autoFocus onClick={onClose}>
      {t("common.ok")}
    </Button>
  );
  const ejectCardOfImport = () => {
    setEject("ejecting");
    ejectCard(jobId).then(
      (result) => {
        setEject(result.status === "ok" ? "ejected" : "refused");
      },
      (error: unknown) => {
        console.error("Failed to eject the card", error);
        setEject("refused");
      },
    );
  };

  switch (status.status) {
    case "completed":
      if (status.result.kind !== "import") return null;
      return (
        <Modal
          title={t("importResult.completedTitle", { folder: folderName })}
          actions={
            <>
              {card !== null && eject !== "ejected" && (
                <Button
                  disabled={eject === "ejecting"}
                  onClick={ejectCardOfImport}
                >
                  {t("importResult.eject", { card })}
                </Button>
              )}
              {okButton}
            </>
          }
        >
          <SummaryLines summary={status.result.summary} />
          {card !== null && eject === "ejected" && (
            <p className={styles.text}>{t("importResult.ejected", { card })}</p>
          )}
          {card !== null && eject === "refused" && (
            <p className={styles.warning}>
              {t("importResult.ejectRefused", { card })}
            </p>
          )}
        </Modal>
      );
    case "failed":
      return (
        <Modal title={t("importResult.failedTitle")} actions={okButton}>
          <p className={styles.text}>{jobFailureMessage(t, status.failure)}</p>
        </Modal>
      );
    case "crashed":
      return (
        <Modal title={t("importResult.failedTitle")} actions={okButton}>
          <p className={styles.text}>{t("importResult.crashed")}</p>
        </Modal>
      );
  }
}

function SummaryLines({ summary }: { summary: ImportSummary }) {
  const { t } = useTranslation();
  const added = summary.addedPhotos + summary.addedVideos;

  return (
    <>
      {summary.destination !== null && added > 0 && (
        <>
          <p className={styles.text}>{t("importResult.copiedTo")}</p>
          <ul className={styles.paths}>
            <li>{summary.destination}</li>
          </ul>
        </>
      )}
      {summary.backupFolder !== null && added > 0 && (
        <>
          <p className={styles.text}>{t("importResult.backedUpTo")}</p>
          <ul className={styles.paths}>
            <li>{summary.backupFolder}</li>
          </ul>
        </>
      )}
      <ul className={styles.figures}>
        {added === 0 && <li>{t("importResult.nothingAdded")}</li>}
        {summary.addedPhotos > 0 && (
          <li>{t("importResult.addedPhotos", counted(summary.addedPhotos))}</li>
        )}
        {summary.addedVideos > 0 && (
          <li>{t("importResult.addedVideos", counted(summary.addedVideos))}</li>
        )}
        {summary.withRecoveredKeywords > 0 && (
          <li>
            {t(
              "importResult.recoveredKeywords",
              counted(summary.withRecoveredKeywords),
            )}
          </li>
        )}
        {added > 0 && summary.templateName !== null && (
          <li>
            {t("importResult.templateApplied", { name: summary.templateName })}
          </li>
        )}
        {summary.alreadyInCatalog > 0 && (
          <li className={styles.secondary}>
            {t(
              "importResult.alreadyInCatalog",
              counted(summary.alreadyInCatalog),
            )}
          </li>
        )}
        {summary.duplicatesSkipped > 0 && (
          <li className={styles.secondary}>
            {t(
              "importResult.duplicatesSkipped",
              counted(summary.duplicatesSkipped),
            )}
          </li>
        )}
        {summary.withoutExifDate > 0 && (
          <li className={styles.secondary}>
            {t(
              "importResult.withoutExifDate",
              counted(summary.withoutExifDate),
            )}
          </li>
        )}
        {summary.unreadableMetadata > 0 && (
          <li className={styles.secondary}>
            {t(
              "importResult.unreadableMetadata",
              counted(summary.unreadableMetadata),
            )}
          </li>
        )}
        {summary.metadataConflicts > 0 && (
          <li className={styles.secondary}>
            {t(
              "importResult.metadataConflicts",
              counted(summary.metadataConflicts),
            )}
          </li>
        )}
      </ul>
      <UnreadableLines
        count={summary.unreadableCount}
        examples={summary.unreadableExamples}
        messageKey="importResult.unreadable"
      />
      <UnreadableLines
        count={summary.copyFailedCount}
        examples={summary.copyFailedExamples}
        messageKey="importResult.copyFailed"
      />
      <UnreadableLines
        count={summary.backupFailedCount}
        examples={summary.backupFailedExamples}
        messageKey="importResult.backupFailed"
      />
      {summary.sidecarsNotCopiedCount > 0 && (
        <p className={styles.warning}>
          {t(
            "importResult.sidecarsNotCopied",
            counted(summary.sidecarsNotCopiedCount),
          )}
        </p>
      )}
    </>
  );
}
