import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import {
  fetchFolderSync,
  importNewFiles,
  refreshReplacedFiles,
} from "../../services/synchronization";
import { useBackendData } from "../../state/useBackendData";
import styles from "./FolderDeleteDialog.module.css";

/**
 * Synchroniser un dossier : les fichiers nouveaux, disparus et
 * modifiés depuis l'import, et ce qu'il faut en faire, coché par défaut :
 * importer les nouveaux, retirer les disparus du catalogue (Cmd+Z), refaire
 * la vignette des modifiés et relire leurs métadonnées.
 */
export function SyncFolderDialog({
  folderId,
  folderName,
  onRemove,
  onReadMetadata,
  onDone,
  onClose,
}: {
  folderId: number;
  folderName: string;
  onRemove: (mediaIds: number[]) => void;
  onReadMetadata: (mediaIds: number[]) => void;
  /** Appelé une fois les changements lancés. */
  onDone: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const load = useCallback(() => fetchFolderSync(folderId), [folderId]);
  const loaded = useBackendData(load, "folder comparison");
  const [importNew, setImportNew] = useState(true);
  const [removeMissing, setRemoveMissing] = useState(true);
  const [readModified, setReadModified] = useState(true);
  const [importFailed, setImportFailed] = useState(false);

  const sync = loaded.status === "ready" ? loaded.data : null;
  const isUpToDate =
    sync !== null &&
    sync.newCount === 0 &&
    sync.missingIds.length === 0 &&
    sync.modifiedIds.length === 0;

  const apply = () => {
    if (sync === null) return;
    if (removeMissing && sync.missingIds.length > 0) {
      onRemove(sync.missingIds);
    }
    if (readModified && sync.modifiedIds.length > 0) {
      const modified = sync.modifiedIds;
      refreshReplacedFiles(modified)
        .catch((error: unknown) => {
          console.error("Failed to refresh the modified files", error);
        })
        .finally(() => {
          onReadMetadata(modified);
        });
    }
    if (!importNew || sync.newCount === 0) {
      onDone();
      return;
    }
    importNewFiles(folderId).then(
      (result) => {
        if (result.status === "ok") onDone();
        else {
          console.error("Failed to start the import", result.error);
          setImportFailed(true);
        }
      },
      (error: unknown) => {
        console.error("Failed to reach the core", error);
        setImportFailed(true);
      },
    );
  };

  const section = (
    count: number,
    examples: readonly string[],
    label: string,
    checked: boolean,
    onCheck: (checked: boolean) => void,
  ) =>
    count > 0 && (
      <>
        <label>
          <input
            type="checkbox"
            checked={checked}
            onChange={(event) => {
              onCheck(event.currentTarget.checked);
            }}
          />{" "}
          {label}
        </label>
        <ul className={styles.names}>
          {examples.map((name, index) => (
            <li key={`${String(index)}-${name}`}>{name}</li>
          ))}
          {count > examples.length && (
            <li className={styles.more}>
              {t("deleteRejected.more", counted(count - examples.length))}
            </li>
          )}
        </ul>
      </>
    );

  return (
    <Modal
      title={t("synchronization.title", { name: folderName })}
      actions={
        isUpToDate || loaded.status === "error" ? (
          <Button variant="primary" autoFocus onClick={onClose}>
            {t("common.ok")}
          </Button>
        ) : (
          <>
            <Button autoFocus onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button variant="primary" disabled={sync === null} onClick={apply}>
              {t("synchronization.apply")}
            </Button>
          </>
        )
      }
    >
      {loaded.status === "loading" && <p>{t("synchronization.loading")}</p>}
      {loaded.status === "error" && (
        <p role="alert">{t("synchronization.loadFailed")}</p>
      )}
      {isUpToDate && <p>{t("synchronization.upToDate")}</p>}
      {sync !== null && !isUpToDate && (
        <>
          {section(
            sync.newCount,
            sync.newExamples,
            t("synchronization.importNew", counted(sync.newCount)),
            importNew,
            setImportNew,
          )}
          {section(
            sync.missingIds.length,
            sync.missingExamples,
            t("synchronization.removeMissing", counted(sync.missingIds.length)),
            removeMissing,
            setRemoveMissing,
          )}
          {section(
            sync.modifiedIds.length,
            sync.modifiedExamples,
            t("synchronization.readModified", counted(sync.modifiedIds.length)),
            readModified,
            setReadModified,
          )}
          <p className={styles.explanation}>
            {t("synchronization.explanation")}
          </p>
        </>
      )}
      {importFailed && <p role="alert">{t("synchronization.importFailed")}</p>}
    </Modal>
  );
}
