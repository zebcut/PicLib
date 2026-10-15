import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import { fetchRejected, trashRejected } from "../../services/deletion";
import type { RejectedSummary } from "../../services/deletion";
import type { GridFilter, GridSource } from "../../services/library";
import styles from "./DeleteRejectedDialog.module.css";

type Loaded =
  | { status: "loading" }
  | { status: "ready"; summary: RejectedSummary }
  | { status: "error" };

/**
 * « Supprimer les photos rejetées » : liste des photos rejetées
 * de la source affichée (filtre compris), puis, après confirmation, retrait du catalogue ou
 * mise à la corbeille.
 *
 * @param onRemove retire les photos du catalogue (annulable).
 * @param onDone appelé après le retrait, ou une fois la mise à la corbeille
 *   lancée (sa progression s'affiche dans l'indicateur des tâches).
 */
export function DeleteRejectedDialog({
  source,
  filter,
  onRemove,
  onDone,
  onClose,
}: {
  source: GridSource;
  filter: GridFilter;
  onRemove: (source: GridSource, filter: GridFilter) => void;
  onDone: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });
  const [isStarting, setIsStarting] = useState(false);
  const [startFailed, setStartFailed] = useState(false);

  useEffect(() => {
    let isActive = true;
    fetchRejected(source, filter).then(
      (summary) => {
        if (isActive) setLoaded({ status: "ready", summary });
      },
      (error: unknown) => {
        console.error("Failed to list the rejected photos", error);
        if (isActive) setLoaded({ status: "error" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [source, filter]);

  const remove = () => {
    onRemove(source, filter);
    onDone();
  };
  const trash = () => {
    setIsStarting(true);
    trashRejected(source, filter).then(
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

  const summary = loaded.status === "ready" ? loaded.summary : null;
  const canDelete = summary !== null && summary.count > 0 && !isStarting;
  return (
    <Modal
      title={t("deleteRejected.title")}
      actions={
        <>
          <Button autoFocus onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button disabled={!canDelete} onClick={remove}>
            {t("deleteRejected.remove")}
          </Button>
          <Button variant="primary" disabled={!canDelete} onClick={trash}>
            {t("deleteRejected.trash")}
          </Button>
        </>
      }
    >
      {loaded.status === "loading" && <p>{t("deleteRejected.loading")}</p>}
      {loaded.status === "error" && (
        <p role="alert" className={styles.error}>
          {t("deleteRejected.loadFailed")}
        </p>
      )}
      {summary?.count === 0 && <p>{t("deleteRejected.none")}</p>}
      {summary !== null && summary.count > 0 && (
        <RejectedList summary={summary} />
      )}
      {startFailed && (
        <p role="alert" className={styles.error}>
          {t("deleteRejected.startFailed")}
        </p>
      )}
    </Modal>
  );
}

function RejectedList({ summary }: { summary: RejectedSummary }) {
  const { t } = useTranslation();
  const more = summary.count - summary.fileNames.length;

  return (
    <>
      <p>{t("deleteRejected.count", counted(summary.count))}</p>
      <ul className={styles.names} aria-label={t("deleteRejected.title")}>
        {summary.fileNames.map((name, index) => (
          // Deux fichiers de dossiers différents peuvent porter le même nom.
          <li key={`${String(index)}-${name}`}>{name}</li>
        ))}
        {more > 0 && (
          <li className={styles.more}>
            {t("deleteRejected.more", counted(more))}
          </li>
        )}
      </ul>
      <p className={styles.explanation}>{t("deleteRejected.explanation")}</p>
    </>
  );
}
