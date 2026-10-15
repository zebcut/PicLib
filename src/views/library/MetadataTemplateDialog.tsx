import type { TFunction } from "i18next";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import type { KeywordListItemDto } from "../../services/keywordList";
import type { MetadataFieldDto } from "../../services/metadata";
import type {
  MetadataTemplateDto,
  MetadataTemplateOperation,
  MetadataTemplateRefusal,
  TemplateInputDto,
} from "../../services/metadataTemplates";
import { itemPath } from "../../utils/tree";
import { KeywordCombobox } from "./KeywordCombobox";
import styles from "./MetadataTemplates.module.css";

/** Champs d'un modèle, dans l'ordre du panneau Métadonnées. */
const FIELDS: readonly MetadataFieldDto[] = [
  "title",
  "caption",
  "creator",
  "copyright",
  "country",
  "region",
  "city",
  "location",
];

/** Ce qu'un modèle fait d'un champ. */
type Mode = "keep" | "fill" | "clear";

interface FieldForm {
  mode: Mode;
  text: string;
}

/** Contenu de la fenêtre pour un modèle. */
interface TemplateForm {
  name: string;
  fields: Record<MetadataFieldDto, FieldForm>;
  keywords: string;
}

/** Valeur spéciale de la liste des modèles : un nouveau modèle. */
const NEW_TEMPLATE = "new";

/**
 * Fenêtre des modèles de métadonnées : choisir le modèle à
 * modifier (ou en créer un), son nom, pour chaque champ « ne rien
 * changer », « remplir » ou « vider », et les mots-clés à ajouter. Ce sont
 * des réglages : ils ne passent pas par Cmd+Z.
 *
 * @param initialId modèle ouvert au départ ; `null` : un nouveau.
 * @param onChange enregistre ou supprime ; renvoie la raison d'un refus,
 *   ou `null` (la fenêtre se ferme alors).
 */
export function MetadataTemplateDialog({
  templates,
  byId,
  initialId,
  onChange,
  onClose,
}: {
  templates: readonly MetadataTemplateDto[];
  byId: ReadonlyMap<number, KeywordListItemDto>;
  initialId: number | null;
  onChange: (
    operation: MetadataTemplateOperation,
  ) => Promise<MetadataTemplateRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorId = useId();
  const [editingId, setEditingId] = useState(initialId);
  const edited = templates.find(({ id }) => id === editingId) ?? null;
  const [form, setForm] = useState(() => formFor(edited, byId));
  const [isDirty, setIsDirty] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [isBusy, setIsBusy] = useState(false);
  const [confirmDelete, setConfirmDelete] = useState(false);

  const update = (next: TemplateForm) => {
    setForm(next);
    setIsDirty(true);
    setProblem(null);
  };
  const setField = (field: MetadataFieldDto, change: Partial<FieldForm>) => {
    update({
      ...form,
      fields: { ...form.fields, [field]: { ...form.fields[field], ...change } },
    });
  };
  const run = (operation: MetadataTemplateOperation) => {
    setIsBusy(true);
    onChange(operation)
      .then((refusal) => {
        setIsBusy(false);
        if (refusal === null) onClose();
        else setProblem(refusalText(t, refusal));
      })
      .catch((error: unknown) => {
        console.error("Failed to change the templates", error);
        setIsBusy(false);
        setProblem(t("metadataTemplates.failed"));
      });
  };

  return (
    <Modal
      title={t("metadataTemplates.dialogTitle")}
      actions={
        <>
          {edited !== null && (
            <Button
              className={styles.delete}
              disabled={isBusy}
              onClick={() => {
                if (confirmDelete) {
                  run({ kind: "delete", templateId: edited.id });
                } else setConfirmDelete(true);
              }}
            >
              {confirmDelete
                ? t("metadataTemplates.confirmDelete")
                : t("metadataTemplates.delete")}
            </Button>
          )}
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={isBusy || form.name.trim() === ""}
            onClick={() => {
              run({
                kind: "save",
                templateId: editingId,
                template: inputOf(form),
              });
            }}
          >
            {t("metadataTemplates.save")}
          </Button>
        </>
      }
    >
      <div
        className={styles.form}
        onKeyDown={(event) => {
          // Échap ferme la fenêtre, sauf s'il ferme d'abord une liste de
          // suggestions (qui arrête alors l'événement).
          if (event.key === "Escape") onClose();
        }}
      >
        <label className={styles.row}>
          <span className={styles.label}>
            {t("metadataTemplates.template")}
          </span>
          <select
            className={styles.input}
            value={editingId ?? NEW_TEMPLATE}
            disabled={isDirty}
            title={isDirty ? t("metadataTemplates.saveFirst") : undefined}
            onChange={(event) => {
              const id =
                event.target.value === NEW_TEMPLATE
                  ? null
                  : Number(event.target.value);
              setEditingId(id);
              setForm(
                formFor(
                  templates.find((other) => other.id === id) ?? null,
                  byId,
                ),
              );
              setConfirmDelete(false);
              setProblem(null);
            }}
          >
            {templates.map((template) => (
              <option key={template.id} value={template.id}>
                {template.name}
              </option>
            ))}
            <option value={NEW_TEMPLATE}>
              {t("metadataTemplates.newTemplate")}
            </option>
          </select>
        </label>
        <label className={styles.row}>
          <span className={styles.label}>{t("metadataTemplates.name")}</span>
          <input
            className={styles.input}
            type="text"
            autoFocus
            spellCheck={false}
            value={form.name}
            aria-describedby={problem === null ? undefined : errorId}
            onChange={(event) => {
              update({ ...form, name: event.target.value });
            }}
          />
        </label>
        <p className={styles.hint}>{t("metadataTemplates.fieldsHint")}</p>
        {FIELDS.map((field) => {
          const label = t(`metadata.fields.${field}`);
          const { mode, text } = form.fields[field];
          return (
            <div key={field} className={styles.row}>
              <span className={styles.label}>{label}</span>
              <select
                className={styles.mode}
                aria-label={t("metadataTemplates.modeOf", { field: label })}
                value={mode}
                onChange={(event) => {
                  setField(field, { mode: event.target.value as Mode });
                }}
              >
                <option value="keep">
                  {t("metadataTemplates.modes.keep")}
                </option>
                <option value="fill">
                  {t("metadataTemplates.modes.fill")}
                </option>
                <option value="clear">
                  {t("metadataTemplates.modes.clear")}
                </option>
              </select>
              <input
                className={styles.input}
                type="text"
                aria-label={t("metadataTemplates.valueOf", { field: label })}
                disabled={mode !== "fill"}
                value={mode === "fill" ? text : ""}
                onChange={(event) => {
                  setField(field, { text: event.target.value });
                }}
              />
            </div>
          );
        })}
        <div className={styles.row}>
          <span className={styles.label}>
            {t("metadataTemplates.keywords")}
          </span>
          <div className={styles.keywords}>
            <KeywordCombobox
              multiple
              value={form.keywords}
              onChange={(keywords) => {
                update({ ...form, keywords });
              }}
              label={t("metadataTemplates.keywords")}
              placeholder={t("keywords.entryPlaceholder")}
            />
          </div>
        </div>
        {problem !== null && (
          <p id={errorId} className={styles.error} role="alert">
            {problem}
          </p>
        )}
      </div>
    </Modal>
  );
}

/** Contenu de la fenêtre pour un modèle existant, ou vide. */
function formFor(
  template: MetadataTemplateDto | null,
  byId: ReadonlyMap<number, KeywordListItemDto>,
): TemplateForm {
  const fields = Object.fromEntries(
    FIELDS.map((field) => {
      const found = template?.fields.find((other) => other.field === field);
      const form: FieldForm =
        found === undefined
          ? { mode: "keep", text: "" }
          : found.value === null
            ? { mode: "clear", text: "" }
            : { mode: "fill", text: found.value };
      return [field, form];
    }),
  ) as Record<MetadataFieldDto, FieldForm>;
  return {
    name: template?.name ?? "",
    fields,
    // Chemins complets : ils désignent exactement ces mots-clés.
    keywords: (template?.keywordIds ?? [])
      .filter((id) => byId.has(id))
      .map((id) => itemPath(byId, id))
      .join(", "),
  };
}

function inputOf(form: TemplateForm): TemplateInputDto {
  return {
    name: form.name,
    fields: FIELDS.flatMap((field) => {
      const { mode, text } = form.fields[field];
      if (mode === "keep") return [];
      return [{ field, value: mode === "fill" ? text : null }];
    }),
    keywords: form.keywords,
  };
}

function refusalText(t: TFunction, refusal: MetadataTemplateRefusal): string {
  switch (refusal.kind) {
    case "notFound":
      return t("metadataTemplates.refusals.notFound");
    case "emptyName":
      return t("metadataTemplates.refusals.emptyName");
    case "tooLong":
      return t("metadataTemplates.refusals.tooLong");
    case "nameTaken":
      return t("metadataTemplates.refusals.nameTaken", { name: refusal.name });
    case "emptyValue":
      return t("metadataTemplates.refusals.emptyValue", {
        field: t(`metadata.fields.${refusal.field}`),
      });
    case "valueTooLong":
      return t("metadataTemplates.refusals.valueTooLong", {
        field: t(`metadata.fields.${refusal.field}`),
      });
    case "ambiguousKeyword":
      return t("keywords.ambiguous", { name: refusal.name });
    case "keywordTooLong":
      return t("keywords.tooLong");
    case "forbiddenCharacter":
      return t("keywords.forbidden", { name: refusal.name });
  }
}
