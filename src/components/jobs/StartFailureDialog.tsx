import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import type { FileMetadataLaunchFailure } from "../../state/useFileMetadataReading";
import { Button } from "../Button";
import { Modal } from "../Modal";
import styles from "./JobDialogs.module.css";

/**
 * Explique pourquoi une lecture des métadonnées des fichiers n'a pas pu
 * démarrer (l'écran d'import explique lui-même ses échecs).
 */
export function StartFailureDialog({
  fileMetadataFailure,
  onDismissFileMetadata,
}: {
  fileMetadataFailure: FileMetadataLaunchFailure | null;
  onDismissFileMetadata: () => void;
}) {
  const { t } = useTranslation();
  if (fileMetadataFailure === null) return null;

  return (
    <Modal
      title={t("fileMetadata.startFailedTitle")}
      actions={
        <Button variant="primary" autoFocus onClick={onDismissFileMetadata}>
          {t("common.ok")}
        </Button>
      }
    >
      <p className={styles.text}>
        {fileMetadataMessage(t, fileMetadataFailure)}
      </p>
    </Modal>
  );
}
function fileMetadataMessage(
  t: TFunction,
  failure: FileMetadataLaunchFailure,
): string {
  switch (failure.kind) {
    case "coreUnreachable":
      return t("folderAnalysis.coreUnreachable");
    case "catalogUnavailable":
      return t("importResult.catalogUnavailable");
    case "jobNotStarted":
      return t("fileMetadata.jobNotStarted");
  }
}
