import { useCallback } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { formatCount } from "../../i18n/format";
import { fetchFolderRelocation } from "../../services/relocation";
import { useBackendData } from "../../state/useBackendData";
import styles from "./FolderDeleteDialog.module.css";

/**
 * Relocaliser un dossier : combien de ses fichiers sont retrouvés
 * au nouvel emplacement, à confirmer ; Cmd+Z annule.
 */
export function RelocateFolderDialog({
  folderId,
  folderName,
  path,
  onRelocate,
  onClose,
}: {
  folderId: number;
  folderName: string;
  /** Nouvel emplacement choisi. */
  path: string;
  onRelocate: (folderId: number, path: string) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const load = useCallback(
    () => fetchFolderRelocation(folderId, path),
    [folderId, path],
  );
  const loaded = useBackendData(load, "folder relocation");
  const preview =
    loaded.status === "ready" && loaded.data.status === "ok"
      ? loaded.data.preview
      : null;

  return (
    <Modal
      title={t("relocation.folderTitle", { name: folderName })}
      actions={
        <>
          <Button autoFocus onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            disabled={preview === null}
            onClick={() => {
              onRelocate(folderId, path);
              onClose();
            }}
          >
            {t("relocation.relocate")}
          </Button>
        </>
      }
    >
      {loaded.status === "loading" && <p>{t("relocation.folderLoading")}</p>}
      {loaded.status === "error" && (
        <p role="alert">{t("relocation.loadFailed")}</p>
      )}
      {loaded.status === "ready" && loaded.data.status === "refused" && (
        <p role="alert">{t(`relocation.refusal.${loaded.data.refusal}`)}</p>
      )}
      {preview !== null && (
        <>
          <p>
            {t("relocation.folderFound", {
              found: formatCount(preview.found),
              total: formatCount(preview.total),
              path,
            })}
          </p>
          <p className={styles.explanation}>
            {t("relocation.folderExplanation")}
          </p>
        </>
      )}
    </Modal>
  );
}
