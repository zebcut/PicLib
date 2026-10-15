import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import { fetchPhotoDeletion, trashPhotos } from "../../services/deletion";
import { useBackendData } from "../../state/useBackendData";
import styles from "./DeleteRejectedDialog.module.css";

/**
 * Suppr : les photos sélectionnées, une pile repliée partant en
 * entier, puis, après confirmation, retrait du catalogue (Cmd+Z) ou mise à
 * la corbeille avec leurs fichiers annexes.
 *
 * @param onRemove retire les photos du catalogue (annulable).
 * @param onDone appelé après le retrait, ou une fois la mise à la corbeille
 *   lancée (sa progression s'affiche dans l'indicateur des tâches).
 */
export function DeletePhotosDialog({
  mediaIds,
  onRemove,
  onDone,
  onClose,
}: {
  mediaIds: readonly number[];
  onRemove: (mediaIds: number[]) => void;
  onDone: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const load = useCallback(() => fetchPhotoDeletion([...mediaIds]), [mediaIds]);
  const loaded = useBackendData(load, "photos to delete");
  const [isStarting, setIsStarting] = useState(false);
  const [startFailed, setStartFailed] = useState(false);

  const trash = () => {
    setIsStarting(true);
    trashPhotos([...mediaIds]).then(
      (result) => {
        setIsStarting(false);
        if (result.status === "ok") onDone();
        else {
          console.error("Failed to start moving to the trash", result.error);
          setStartFailed(true);
        }
      },
      (error: unknown) => {
        console.error("Failed to reach the core", error);
        setIsStarting(false);
        setStartFailed(true);
      },
    );
  };

  const summary = loaded.status === "ready" ? loaded.data : null;
  const canDelete = summary !== null && summary.count > 0 && !isStarting;
  const more = summary === null ? 0 : summary.count - summary.fileNames.length;
  return (
    <Modal
      title={t("deletePhotos.title")}
      actions={
        <>
          <Button autoFocus onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button
            disabled={!canDelete}
            onClick={() => {
              onRemove([...mediaIds]);
              onDone();
            }}
          >
            {t("deleteRejected.remove")}
          </Button>
          <Button variant="primary" disabled={!canDelete} onClick={trash}>
            {t("deleteRejected.trash")}
          </Button>
        </>
      }
    >
      {loaded.status === "loading" && <p>{t("deletePhotos.loading")}</p>}
      {loaded.status === "error" && (
        <p role="alert" className={styles.error}>
          {t("deletePhotos.loadFailed")}
        </p>
      )}
      {summary !== null && (
        <>
          <p>{t("deletePhotos.count", counted(summary.count))}</p>
          <ul className={styles.names} aria-label={t("deletePhotos.title")}>
            {summary.fileNames.map((name, index) => (
              <li key={`${String(index)}-${name}`}>{name}</li>
            ))}
            {more > 0 && (
              <li className={styles.more}>
                {t("deleteRejected.more", counted(more))}
              </li>
            )}
          </ul>
          {summary.fromStacks > 0 && (
            <p>{t("deletePhotos.fromStacks", counted(summary.fromStacks))}</p>
          )}
          <p className={styles.explanation}>{t("deletePhotos.explanation")}</p>
        </>
      )}
      {startFailed && (
        <p role="alert" className={styles.error}>
          {t("deleteRejected.startFailed")}
        </p>
      )}
    </Modal>
  );
}
