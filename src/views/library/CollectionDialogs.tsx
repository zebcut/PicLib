import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import { fetchCollectionDeletion } from "../../services/collections";
import type {
  CollectionDeletionDto,
  CollectionRefusal,
} from "../../services/collections";
import styles from "./KeywordListPanel.module.css";

/**
 * Nom d'une collection ou d'un dossier à créer ou à renommer. À la création
 * d'une collection, « Ajouter les photos sélectionnées » est cochée s'il y
 * en a.
 *
 * @param selectedCount photos sélectionnées ; `null` : la case n'est pas
 *   proposée (dossier, renommage).
 * @param onSubmit renvoie la raison d'un refus, ou `null` (la fenêtre est
 *   alors fermée par l'appelant).
 */
export function CollectionNameDialog({
  title,
  initialName,
  selectedCount,
  onSubmit,
  onClose,
}: {
  title: string;
  initialName: string;
  selectedCount: number | null;
  onSubmit: (
    name: string,
    includeSelected: boolean,
  ) => Promise<CollectionRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorId = useId();
  const [name, setName] = useState(initialName);
  const [includeSelected, setIncludeSelected] = useState(true);
  const [refusal, setRefusal] = useState<CollectionRefusal | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const submit = () => {
    if (isSaving || name.trim() === "") return;
    setIsSaving(true);
    onSubmit(name, includeSelected && (selectedCount ?? 0) > 0)
      .then((result) => {
        setRefusal(result);
        setIsSaving(false);
      })
      .catch((error: unknown) => {
        console.error("Failed to save the collection", error);
        setIsSaving(false);
      });
  };

  return (
    <Modal
      title={title}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={name.trim() === "" || isSaving}
            onClick={submit}
          >
            {t("common.ok")}
          </Button>
        </>
      }
    >
      <div className={styles.form}>
        <label className={styles.field}>
          {t("collections.name")}
          <input
            className={styles.input}
            type="text"
            autoFocus
            spellCheck={false}
            value={name}
            aria-describedby={refusal === null ? undefined : errorId}
            onChange={(event) => {
              setName(event.target.value);
              setRefusal(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") submit();
              else if (event.key === "Escape") onClose();
            }}
          />
        </label>
        {selectedCount !== null && selectedCount > 0 && (
          <label className={styles.checkboxField}>
            <input
              type="checkbox"
              checked={includeSelected}
              onChange={(event) => {
                setIncludeSelected(event.target.checked);
              }}
            />
            {t("collections.includeSelected", counted(selectedCount))}
          </label>
        )}
        {refusal !== null && (
          <p id={errorId} className={styles.error} role="alert">
            {t(`collections.refusal.${refusal}`)}
          </p>
        )}
      </div>
    </Modal>
  );
}

/**
 * Confirmation de la suppression d'un dossier qui contient des collections
 * : ce qu'il contient, et que les photos restent.
 */
export function DeleteCollectionFolderDialog({
  collectionId,
  name,
  onConfirm,
  onClose,
}: {
  collectionId: number;
  name: string;
  onConfirm: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [summary, setSummary] = useState<CollectionDeletionDto | null>(null);

  useEffect(() => {
    let isActive = true;
    fetchCollectionDeletion(collectionId).then(
      (loaded) => {
        if (isActive) setSummary(loaded);
      },
      (error: unknown) => {
        console.error("Failed to read the collection folder", error);
      },
    );
    return () => {
      isActive = false;
    };
  }, [collectionId]);

  return (
    <Modal
      title={t("collections.deleteTitle", { name })}
      actions={
        <>
          <Button autoFocus onClick={onClose}>
            {t("common.cancel")}
          </Button>
          <Button variant="primary" onClick={onConfirm}>
            {t("collections.delete")}
          </Button>
        </>
      }
    >
      {summary !== null && (
        <p>
          {t("collections.deleteContains", {
            collections: t(
              "collections.collectionsCount",
              counted(summary.collections),
            ),
            // Le dossier lui-même n'est pas compté parmi ce qu'il contient.
            folders: t(
              "collections.foldersCount",
              counted(Math.max(0, summary.folders - 1)),
            ),
          })}
        </p>
      )}
      <p className={styles.hint}>{t("collections.deleteExplanation")}</p>
    </Modal>
  );
}

/**
 * « Enregistrer la collection rapide… » (Cmd+Option+B) : le nom de la
 * collection créée à la racine, et « Vider la collection rapide ensuite »,
 * cochée comme dans Lightroom.
 *
 * @param onSave renvoie la raison d'un refus, ou `null` (la fenêtre est
 *   alors fermée par l'appelant).
 */
export function QuickCollectionSaveDialog({
  onSave,
  onClose,
}: {
  onSave: (name: string, clear: boolean) => Promise<CollectionRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorId = useId();
  const [name, setName] = useState("");
  const [clear, setClear] = useState(true);
  const [refusal, setRefusal] = useState<CollectionRefusal | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const submit = () => {
    if (isSaving || name.trim() === "") return;
    setIsSaving(true);
    onSave(name, clear)
      .then((result) => {
        setRefusal(result);
        setIsSaving(false);
      })
      .catch((error: unknown) => {
        console.error("Failed to save the quick collection", error);
        setIsSaving(false);
      });
  };

  return (
    <Modal
      title={t("collections.quickSaveTitle")}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={name.trim() === "" || isSaving}
            onClick={submit}
          >
            {t("common.ok")}
          </Button>
        </>
      }
    >
      <div className={styles.form}>
        <label className={styles.field}>
          {t("collections.name")}
          <input
            className={styles.input}
            type="text"
            autoFocus
            spellCheck={false}
            value={name}
            aria-describedby={refusal === null ? undefined : errorId}
            onChange={(event) => {
              setName(event.target.value);
              setRefusal(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") submit();
              else if (event.key === "Escape") onClose();
            }}
          />
        </label>
        <label className={styles.checkboxField}>
          <input
            type="checkbox"
            checked={clear}
            onChange={(event) => {
              setClear(event.target.checked);
            }}
          />
          {t("collections.clearAfter")}
        </label>
        {refusal !== null && (
          <p id={errorId} className={styles.error} role="alert">
            {t(`collections.refusal.${refusal}`)}
          </p>
        )}
      </div>
    </Modal>
  );
}
