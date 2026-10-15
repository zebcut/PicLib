import { useCallback, useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import {
  deleteRenameTemplate,
  fetchRenamePreview,
  fetchRenameTemplates,
  saveRenameTemplate,
} from "../../services/mediaMove";
import type { RenameTemplate } from "../../services/mediaMove";
import { useBackendData } from "../../state/useBackendData";
import { useThrottledValue } from "../../state/useThrottledValue";
import styles from "./RenameDialog.module.css";
import { RenameTemplateEditor } from "./RenameTemplateEditor";

/**
 * Modèle de départ : pour une photo, son nom actuel à modifier ; pour
 * plusieurs, « Photo- » et un numéro.
 */
export function initialRenameTemplate(
  count: number,
  currentStem: string,
): RenameTemplate {
  return {
    parts:
      count === 1
        ? [{ kind: "text", text: currentStem }]
        : [{ kind: "text", text: "Photo-" }, { kind: "sequence" }],
    sequenceStart: 1,
    sequenceDigits: 3,
  };
}

/**
 * Renommer des photos : un modèle fait d'éléments, son aperçu
 * sur les premières photos, et les modèles enregistrés ; Cmd+Z annule.
 *
 * @param mediaIds photos à renommer, dans l'ordre de la grille (celui de
 *   la séquence).
 */
export function RenameDialog({
  mediaIds,
  initialTemplate,
  onRename,
  onClose,
}: {
  mediaIds: readonly number[];
  initialTemplate: RenameTemplate;
  onRename: (mediaIds: number[], template: RenameTemplate) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const savedId = useId();
  const [template, setTemplate] = useState(initialTemplate);
  const [templatesVersion, setTemplatesVersion] = useState(0);
  const [saveName, setSaveName] = useState("");
  // L'aperçu suit la saisie sans interroger le cœur à chaque touche.
  const shown = useThrottledValue(template, 200);
  const loadPreview = useCallback(
    () => fetchRenamePreview([...mediaIds], shown),
    [mediaIds, shown],
  );
  const preview = useBackendData(loadPreview, "rename preview");
  const templates = useBackendData(
    fetchRenameTemplates,
    "rename templates",
    templatesVersion,
  );
  const saved = templates.status === "ready" ? templates.data : [];
  const data = preview.status === "ready" ? preview.data : null;
  const canRename =
    data !== null && data.refusal === null && shown === template;

  return (
    <Modal
      title={t("rename.title", counted(mediaIds.length))}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={!canRename}
            onClick={() => {
              onRename([...mediaIds], template);
              onClose();
            }}
          >
            {t("rename.apply")}
          </Button>
        </>
      }
    >
      <RenameTemplateEditor template={template} onChange={setTemplate} />
      <div className={styles.row}>
        <label htmlFor={savedId}>{t("rename.saved")}</label>
        <select
          id={savedId}
          value=""
          disabled={saved.length === 0}
          onChange={(event) => {
            const chosen = saved.find(
              (entry) => String(entry.id) === event.currentTarget.value,
            );
            if (chosen !== undefined) {
              setTemplate(chosen.template);
              setSaveName(chosen.name);
            }
          }}
        >
          <option value="">{t("rename.choose")}</option>
          {saved.map((entry) => (
            <option key={entry.id} value={entry.id}>
              {entry.name}
            </option>
          ))}
        </select>
        <input
          type="text"
          aria-label={t("rename.templateName")}
          placeholder={t("rename.templateName")}
          value={saveName}
          onChange={(event) => {
            setSaveName(event.currentTarget.value);
          }}
        />
        <Button
          disabled={saveName.trim() === "" || template.parts.length === 0}
          onClick={() => {
            saveRenameTemplate(saveName, template).then(
              () => {
                setTemplatesVersion((version) => version + 1);
              },
              (error: unknown) => {
                console.error("Failed to save the rename template", error);
              },
            );
          }}
        >
          {t("rename.save")}
        </Button>
        <Button
          disabled={
            !saved.some(
              (entry) =>
                entry.name.toLowerCase() === saveName.trim().toLowerCase(),
            )
          }
          onClick={() => {
            const entry = saved.find(
              (candidate) =>
                candidate.name.toLowerCase() === saveName.trim().toLowerCase(),
            );
            if (entry === undefined) return;
            deleteRenameTemplate(entry.id).then(
              () => {
                setSaveName("");
                setTemplatesVersion((version) => version + 1);
              },
              (error: unknown) => {
                console.error("Failed to delete the rename template", error);
              },
            );
          }}
        >
          {t("rename.delete")}
        </Button>
      </div>
      {preview.status === "error" && (
        <p role="alert">{t("rename.loadFailed")}</p>
      )}
      {data?.refusal != null && (
        <p role="alert">{t(`fileMove.refusal.${data.refusal}`)}</p>
      )}
      {data !== null && data.refusal === null && (
        <>
          <ul className={styles.preview} aria-label={t("rename.preview")}>
            {data.examples.map((example, index) => (
              <li key={`${String(index)}-${example.from}`}>
                {example.from} → {example.to}
              </li>
            ))}
          </ul>
          <p className={styles.secondary}>
            {t("rename.summary", counted(data.mediaCount))}
            {data.sidecarCount > 0 &&
              ` ${t("fileMove.sidecars", counted(data.sidecarCount))}`}{" "}
            {t("rename.explanation")}
          </p>
        </>
      )}
    </Modal>
  );
}
