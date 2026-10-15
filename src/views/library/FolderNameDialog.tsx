import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";

/**
 * Nom d'un dossier à créer ou à renommer ; un refus (nom pris)
 * s'explique ensuite en bas de la fenêtre.
 */
export function FolderNameDialog({
  title,
  initialName,
  explanation,
  submitLabel,
  onSubmit,
  onClose,
}: {
  title: string;
  initialName: string;
  /** Ce que fera la fenêtre, sous le champ. */
  explanation?: string;
  submitLabel: string;
  onSubmit: (name: string) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const inputId = useId();
  const [name, setName] = useState(initialName);
  const isEmpty = name.trim() === "";
  const submit = () => {
    if (isEmpty) return;
    onSubmit(name.trim());
    onClose();
  };

  return (
    <Modal
      title={title}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={isEmpty} onClick={submit}>
            {submitLabel}
          </Button>
        </>
      }
    >
      <label htmlFor={inputId}>{t("fileMove.folderName")}</label>{" "}
      <input
        id={inputId}
        type="text"
        value={name}
        autoFocus
        onChange={(event) => {
          setName(event.currentTarget.value);
        }}
        onKeyDown={(event) => {
          if (event.key === "Enter") submit();
        }}
      />
      {explanation !== undefined && <p>{explanation}</p>}
    </Modal>
  );
}
