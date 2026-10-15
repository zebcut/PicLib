import { useCallback } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import { fetchMediaMove } from "../../services/mediaMove";
import { useBackendData } from "../../state/useBackendData";
import styles from "./FolderDeleteDialog.module.css";

/**
 * Déplacer des photos glissées sur un dossier : le résumé à
 * confirmer ; Cmd+Z les remet à leur place.
 */
export function MoveMediaDialog({
  mediaIds,
  folderId,
  onMove,
  onClose,
}: {
  mediaIds: readonly number[];
  folderId: number;
  onMove: (mediaIds: number[], folderId: number) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const load = useCallback(
    () => fetchMediaMove([...mediaIds], folderId),
    [mediaIds, folderId],
  );
  const loaded = useBackendData(load, "photo move");
  const preview = loaded.status === "ready" ? loaded.data : null;
  const canMove = preview !== null && preview.refusal === null;

  return (
    <Modal
      title={t("fileMove.title", {
        name: preview?.destinationName ?? "",
      })}
      actions={
        <>
          <Button autoFocus onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={!canMove}
            onClick={() => {
              onMove([...mediaIds], folderId);
              onClose();
            }}
          >
            {t("fileMove.move")}
          </Button>
        </>
      }
    >
      {loaded.status === "loading" && <p>{t("fileMove.loading")}</p>}
      {loaded.status === "error" && (
        <p role="alert">{t("fileMove.loadFailed")}</p>
      )}
      {preview?.refusal != null && (
        <p role="alert">{t(`fileMove.refusal.${preview.refusal}`)}</p>
      )}
      {canMove && (
        <>
          <p>
            {t("fileMove.summary", {
              photos: t("fileMove.photos", counted(preview.mediaCount)),
              name: preview.destinationName,
            })}
            {preview.sidecarCount > 0 &&
              ` ${t("fileMove.sidecars", counted(preview.sidecarCount))}`}
          </p>
          <p className={styles.explanation}>{t("fileMove.explanation")}</p>
        </>
      )}
    </Modal>
  );
}
