import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import { fetchFolderMove } from "../../services/folders";
import type {
  FolderMoveDto,
  FolderMoveResultDto,
  MoveRefusalDto,
} from "../../services/folders";
import styles from "./FolderMoveDialog.module.css";

type Loaded =
  | { status: "loading" }
  | { status: "ready"; move: FolderMoveDto }
  | { status: "gone" }
  | { status: "error" };

/**
 * Confirmation du déplacement d'un dossier glissé sur un autre (panneau Dossiers) : le résumé à confirmer, avec
 * le nouveau nom si le sien est pris. Le déplacement s'annule par Cmd+Z.
 *
 * @param onMove déplace le dossier.
 * @param onDone appelé une fois le dossier déplacé.
 */
export function FolderMoveDialog({
  folderId,
  destinationId,
  onMove,
  onDone,
  onClose,
}: {
  folderId: number;
  destinationId: number;
  onMove: (
    folderId: number,
    destinationId: number,
  ) => Promise<FolderMoveResultDto>;
  onDone: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });
  const [isMoving, setIsMoving] = useState(false);
  // Refus constaté au moment du déplacement : le disque a pu changer depuis
  // le résumé.
  const [refusal, setRefusal] = useState<MoveRefusalDto | null>(null);
  const [failed, setFailed] = useState(false);

  useEffect(() => {
    let isActive = true;
    fetchFolderMove(folderId, destinationId).then(
      (move) => {
        if (!isActive) return;
        setLoaded(
          move === null ? { status: "gone" } : { status: "ready", move },
        );
      },
      (error: unknown) => {
        console.error("Failed to check the folder move", error);
        if (isActive) setLoaded({ status: "error" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [folderId, destinationId]);

  const move = () => {
    setIsMoving(true);
    setFailed(false);
    onMove(folderId, destinationId).then(
      (result) => {
        setIsMoving(false);
        switch (result.kind) {
          case "moved":
            onDone();
            break;
          case "refused":
            setRefusal(result.refusal);
            break;
          case "unknown":
            setLoaded({ status: "gone" });
            break;
          case "failed":
            console.error("The folder could not be moved", result.reason);
            setFailed(true);
            break;
        }
      },
      (error: unknown) => {
        console.error("Failed to move the folder", error);
        setIsMoving(false);
        setFailed(true);
      },
    );
  };

  const summary = loaded.status === "ready" ? loaded.move : null;
  const shownRefusal = refusal ?? summary?.refusal ?? null;
  const canMove = summary !== null && shownRefusal === null && !isMoving;
  return (
    <Modal
      title={t("folders.move.title")}
      actions={
        <>
          <Button autoFocus onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" disabled={!canMove} onClick={move}>
            {isMoving ? t("folders.move.moving") : t("folders.move.confirm")}
          </Button>
        </>
      }
    >
      {loaded.status === "loading" && <p>{t("folders.move.loading")}</p>}
      {loaded.status === "error" && (
        <p role="alert" className={styles.error}>
          {t("folders.move.loadFailed")}
        </p>
      )}
      {loaded.status === "gone" && <p>{t("folders.delete.gone")}</p>}
      {summary !== null && <MoveSummary move={summary} />}
      {shownRefusal !== null && (
        <p role="alert" className={styles.error}>
          {t(`folders.move.refusal.${shownRefusal}`)}
        </p>
      )}
      {failed && (
        <p role="alert" className={styles.error}>
          {t("folders.move.failed")}
        </p>
      )}
    </Modal>
  );
}

/** Ce qui sera déplacé, où, et sous quel nom. */
function MoveSummary({ move }: { move: FolderMoveDto }) {
  const { t, i18n } = useTranslation();
  const parts = [
    move.photos > 0 && t("folders.delete.photos", counted(move.photos)),
    move.videos > 0 && t("folders.delete.videos", counted(move.videos)),
    move.subfolders > 0 &&
      t("folders.delete.subfolders", counted(move.subfolders)),
  ].filter((part) => part !== false);

  return (
    <>
      <p>
        {t("folders.move.summary", {
          name: move.name,
          destination: move.destinationName,
          list: new Intl.ListFormat(i18n.language, {
            type: "conjunction",
          }).format(parts),
        })}
      </p>
      <p className={styles.path}>{move.destinationPath}</p>
      {move.targetName !== null && move.targetName !== move.name && (
        <p>
          {t("folders.move.renamed", {
            name: move.name,
            target: move.targetName,
          })}
        </p>
      )}
      <p className={styles.explanation}>{t("folders.move.explanation")}</p>
    </>
  );
}
