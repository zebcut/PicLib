import type { TFunction } from "i18next";
import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import type { KeywordListItemDto } from "../../services/keywordList";
import type { MetadataTemplateDto } from "../../services/metadataTemplates";
import type { MetadataTemplates } from "../../state/useMetadataTemplates";
import { itemPath } from "../../utils/tree";
import { MetadataTemplateDialog } from "./MetadataTemplateDialog";
import styles from "./MetadataTemplates.module.css";

/** Valeurs spéciales de la liste, à côté des identifiants de modèles. */
const NEW_TEMPLATE = "new";
const EDIT_TEMPLATES = "edit";

/**
 * Liste « Modèle » en haut du panneau Métadonnées : choisir un
 * modèle l'applique aux photos sélectionnées (après confirmation au-delà
 * d'une photo) ; « Nouveau modèle… » et « Modifier les modèles… » ouvrent
 * la fenêtre des modèles.
 *
 * @param byId tous les mots-clés, par identifiant (chemins des mots-clés
 *   des modèles).
 * @param onApply applique un modèle aux médias indiqués (annulable).
 */
export function MetadataTemplateBar({
  templates,
  byId,
  selectedIds,
  onApply,
}: {
  templates: MetadataTemplates;
  byId: ReadonlyMap<number, KeywordListItemDto>;
  selectedIds: ReadonlySet<number>;
  onApply: (templateId: number, mediaIds: number[]) => void;
}) {
  const { t } = useTranslation();
  const [confirming, setConfirming] = useState<MetadataTemplateDto | null>(
    null,
  );
  // Fenêtre des modèles : `null` fermée ; sinon le modèle ouvert (`null`
  // pour un nouveau).
  const [editing, setEditing] = useState<{ id: number | null } | null>(null);
  const list =
    templates.state.status === "ready" ? templates.state.templates : [];

  const apply = (template: MetadataTemplateDto) => {
    if (selectedIds.size > 1) setConfirming(template);
    else onApply(template.id, [...selectedIds]);
  };

  return (
    <div className={styles.bar}>
      <select
        className={styles.choice}
        aria-label={t("metadataTemplates.choose")}
        value=""
        onChange={(event) => {
          const { value } = event.target;
          if (value === NEW_TEMPLATE) setEditing({ id: null });
          else if (value === EDIT_TEMPLATES) {
            setEditing({ id: list[0]?.id ?? null });
          } else {
            const template = list.find(({ id }) => String(id) === value);
            if (template !== undefined) apply(template);
          }
        }}
      >
        <option value="">{t("metadataTemplates.placeholder")}</option>
        {list.map((template) => (
          <option
            key={template.id}
            value={template.id}
            disabled={selectedIds.size === 0}
          >
            {template.name}
          </option>
        ))}
        <option disabled>──────────</option>
        <option value={NEW_TEMPLATE}>{t("metadataTemplates.new")}</option>
        <option value={EDIT_TEMPLATES} disabled={list.length === 0}>
          {t("metadataTemplates.edit")}
        </option>
      </select>
      {templates.state.status === "failed" && (
        <p className={styles.error} role="alert">
          {t("metadataTemplates.loadFailed")}
        </p>
      )}
      {confirming !== null && (
        <Modal
          title={t("metadataTemplates.confirmTitle", {
            name: confirming.name,
            ...counted(selectedIds.size),
          })}
          actions={
            <>
              <Button
                onClick={() => {
                  setConfirming(null);
                }}
              >
                {t("common.cancel")}
              </Button>
              <Button
                variant="primary"
                autoFocus
                onClick={() => {
                  onApply(confirming.id, [...selectedIds]);
                  setConfirming(null);
                }}
              >
                {t("metadataTemplates.apply")}
              </Button>
            </>
          }
        >
          <TemplateSummary template={confirming} byId={byId} />
          <p className={styles.hint}>{t("metadataTemplates.undoHint")}</p>
        </Modal>
      )}
      {editing !== null && (
        <MetadataTemplateDialog
          templates={list}
          byId={byId}
          initialId={editing.id}
          onChange={templates.change}
          onClose={() => {
            setEditing(null);
          }}
        />
      )}
    </div>
  );
}

/** Ce que fera un modèle, champ par champ. */
function TemplateSummary({
  template,
  byId,
}: {
  template: MetadataTemplateDto;
  byId: ReadonlyMap<number, KeywordListItemDto>;
}) {
  const { t } = useTranslation();
  const keywords = template.keywordIds
    .filter((id) => byId.has(id))
    .map((id) => itemPath(byId, id));
  if (template.fields.length === 0 && keywords.length === 0) {
    return <p>{t("metadataTemplates.doesNothing")}</p>;
  }
  return (
    <ul className={styles.summary}>
      {template.fields.map(({ field, value }) => (
        <li key={field}>{fieldLine(t, field, value)}</li>
      ))}
      {keywords.length > 0 && (
        <li>
          {t("metadataTemplates.addsKeywords", {
            keywords: keywords.join(", "),
          })}
        </li>
      )}
    </ul>
  );
}

function fieldLine(
  t: TFunction,
  field: MetadataTemplateDto["fields"][number]["field"],
  value: string | null,
): string {
  const label = t(`metadata.fields.${field}`);
  return value === null
    ? t("metadataTemplates.clears", { field: label })
    : t("metadataTemplates.fills", { field: label, value });
}
