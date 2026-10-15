import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted, formatBytes } from "../../i18n/format";
import { fetchFolderDeletion } from "../../services/folders";
import type {
  FolderDeletionDto,
  FolderTrashResultDto,
  TrashCheckDto,
} from "../../services/folders";
import styles from "./FolderDeleteDialog.module.css";

type Loaded =
  | { status: "loading" }
  | { status: "ready"; folder: FolderDeletionDto }
  | { status: "gone" }
  | { status: "error" };

/**
 * « Supprimer le dossier… »(panneau Dossiers) : ce que
 * contient le dossier, sous-dossiers compris, puis « Retirer du catalogue »
 * (annulable) ou « Placer dans la corbeille ». La corbeille est grisée, avec
 * son explication, si le dossier contient des fichiers inconnus du
 * catalogue, s'il est introuvable ou illisible.
 *
 * @param onRemove retire le dossier du catalogue (annulable).
 * @param onTrash place le dossier dans la corbeille.
 * @param onDone appelé une fois le dossier retiré ou mis à la corbeille.
 */
export function FolderDeleteDialog({
  folderId,
  onRemove,
  onTrash,
  onDone,
  onClose,
}: {
  folderId: number;
  onRemove: (folderId: number) => void;
  onTrash: (folderId: number) => Promise<FolderTrashResultDto>;
  onDone: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });
  const [isTrashing, setIsTrashing] = useState(false);
  // Refus constaté au moment de la mise à la corbeille : le dossier a pu
  // changer depuis son résumé.
  const [refusal, setRefusal] = useState<TrashCheckDto | null>(null);
  const [trashFailed, setTrashFailed] = useState(false);

  useEffect(() => {
    let isActive = true;
    fetchFolderDeletion(folderId).then(
      (folder) => {
        if (!isActive) return;
        setLoaded(
          folder === null ? { status: "gone" } : { status: "ready", folder },
        );
      },
      (error: unknown) => {
        console.error("Failed to read the folder to delete", error);
        if (isActive) setLoaded({ status: "error" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [folderId]);

  const remove = () => {
    onRemove(folderId);
    onDone();
  };
  const trash = () => {
    setIsTrashing(true);
    setTrashFailed(false);
    onTrash(folderId).then(
      (result) => {
        setIsTrashing(false);
        switch (result.kind) {
          case "trashed":
            onDone();
            break;
          case "refused":
            setRefusal(result.check);
            break;
          case "unknown":
            setLoaded({ status: "gone" });
            break;
          case "failed":
            console.error("The folder could not be trashed", result.reason);
            setTrashFailed(true);
            break;
        }
      },
      (error: unknown) => {
        console.error("Failed to trash the folder", error);
        setIsTrashing(false);
        setTrashFailed(true);
      },
    );
  };

  const folder = loaded.status === "ready" ? loaded.folder : null;
  const check = refusal ?? folder?.trash ?? null;
  return (
    <Modal
      title={t("folders.delete.title", { name: folder?.name ?? "" })}
      actions={
        <>
          <Button autoFocus onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button disabled={folder === null || isTrashing} onClick={remove}>
            {t("folders.delete.remove")}
          </Button>
          <Button
            variant="primary"
            disabled={check?.kind !== "possible" || isTrashing}
            onClick={trash}
          >
            {isTrashing
              ? t("folders.delete.trashing")
              : t("folders.delete.trash")}
          </Button>
        </>
      }
    >
      {loaded.status === "loading" && <p>{t("folders.delete.loading")}</p>}
      {loaded.status === "error" && (
        <p role="alert" className={styles.error}>
          {t("folders.delete.loadFailed")}
        </p>
      )}
      {loaded.status === "gone" && <p>{t("folders.delete.gone")}</p>}
      {folder !== null && <FolderSummary folder={folder} />}
      {check !== null && <TrashCheckMessage check={check} />}
      {trashFailed && (
        <p role="alert" className={styles.error}>
          {t("folders.delete.failed")}
        </p>
      )}
    </Modal>
  );
}

/** Chemin et contenu du dossier, puis ce que fait chaque bouton. */
function FolderSummary({ folder }: { folder: FolderDeletionDto }) {
  const { t, i18n } = useTranslation();
  const parts = [
    folder.photos > 0 && t("folders.delete.photos", counted(folder.photos)),
    folder.videos > 0 && t("folders.delete.videos", counted(folder.videos)),
    folder.subfolders > 0 &&
      t("folders.delete.subfolders", counted(folder.subfolders)),
  ].filter((part) => part !== false);
  const hasMedia = folder.photos + folder.videos > 0;

  return (
    <>
      <p className={styles.path}>{folder.path}</p>
      <p>
        {hasMedia
          ? t("folders.delete.contains", {
              list: new Intl.ListFormat(i18n.language, {
                type: "conjunction",
              }).format(parts),
              size: formatBytes(folder.sizeBytes),
            })
          : t("folders.delete.nothing")}
      </p>
      <p className={styles.explanation}>{t("folders.delete.explanation")}</p>
    </>
  );
}

/** Pourquoi la corbeille n'est pas possible ; rien si elle l'est. */
function TrashCheckMessage({ check }: { check: TrashCheckDto }) {
  const { t } = useTranslation();

  switch (check.kind) {
    case "possible":
      return null;
    case "offline":
    case "unreadable":
      return (
        <p role="alert" className={styles.error}>
          {t(`folders.delete.${check.kind}`)}
        </p>
      );
    case "unknownFiles": {
      const more = check.count - check.examples.length;
      return (
        <>
          <p role="alert" className={styles.error}>
            {t("folders.delete.unknownFiles", counted(check.count))}
          </p>
          <ul
            className={styles.names}
            aria-label={t("folders.delete.unknownFiles", counted(check.count))}
          >
            {check.examples.map((file) => (
              <li key={file}>{file}</li>
            ))}
            {more > 0 && (
              <li className={styles.more}>
                {t("folders.delete.more", counted(more))}
              </li>
            )}
          </ul>
          <p className={styles.explanation}>
            {t("folders.delete.unknownHint")}
          </p>
        </>
      );
    }
  }
}
