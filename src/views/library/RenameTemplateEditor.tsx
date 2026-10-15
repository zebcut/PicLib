import { useId } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import type {
  RenameDateFormat,
  RenamePart,
  RenameTemplate,
} from "../../services/mediaMove";
import styles from "./RenameDialog.module.css";

/** Formats de date proposés, dans l'ordre du menu. */
const DATE_FORMATS: readonly RenameDateFormat[] = [
  "compact",
  "dashed",
  "compactWithTime",
  "dashedWithTime",
  "year",
];

/** Éléments qu'on peut ajouter, dans l'ordre des boutons. */
export const RENAME_PART_KINDS: readonly RenamePart["kind"][] = [
  "text",
  "captureDate",
  "originalName",
  "currentName",
  "sequence",
  "camera",
];

/** Élément neuf d'une sorte. */
function newPart(kind: RenamePart["kind"]): RenamePart {
  switch (kind) {
    case "text":
      return { kind, text: "" };
    case "captureDate":
      return { kind, format: "compact" };
    default:
      return { kind };
  }
}

/**
 * Modèle de renommage : ses éléments, chacun retirable, les
 * boutons pour en ajouter, et la numérotation s'il y a un numéro. Servi par
 * la fenêtre Renommer et par l'écran d'import.
 *
 * @param kinds éléments proposés, dans l'ordre des boutons.
 */
export function RenameTemplateEditor({
  template,
  onChange,
  kinds = RENAME_PART_KINDS,
}: {
  template: RenameTemplate;
  onChange: (template: RenameTemplate) => void;
  kinds?: readonly RenamePart["kind"][];
}) {
  const { t } = useTranslation();
  const startId = useId();
  const digitsId = useId();
  const setParts = (parts: RenamePart[]) => {
    onChange({ ...template, parts });
  };
  const changePart = (index: number, part: RenamePart) => {
    setParts(template.parts.map((old, at) => (at === index ? part : old)));
  };
  const hasSequence = template.parts.some((part) => part.kind === "sequence");

  return (
    <>
      <ol className={styles.parts} aria-label={t("rename.parts")}>
        {template.parts.map((part, index) => (
          <li key={index} className={styles.part}>
            <PartEditor
              part={part}
              onChange={(next) => {
                changePart(index, next);
              }}
            />
            <button
              type="button"
              aria-label={t("rename.removePart")}
              onClick={() => {
                setParts(template.parts.filter((_, at) => at !== index));
              }}
            >
              ×
            </button>
          </li>
        ))}
      </ol>
      <div className={styles.adders}>
        {kinds.map((kind) => (
          <Button
            key={kind}
            onClick={() => {
              setParts([...template.parts, newPart(kind)]);
            }}
          >
            + {t(`rename.part.${kind}`)}
          </Button>
        ))}
      </div>
      {hasSequence && (
        <div className={styles.row}>
          <label htmlFor={startId}>{t("rename.sequenceStart")}</label>
          <input
            id={startId}
            type="number"
            min={0}
            value={template.sequenceStart}
            onChange={(event) => {
              onChange({
                ...template,
                sequenceStart: Math.max(0, Number(event.currentTarget.value)),
              });
            }}
          />
          <label htmlFor={digitsId}>{t("rename.sequenceDigits")}</label>
          <select
            id={digitsId}
            value={template.sequenceDigits}
            onChange={(event) => {
              onChange({
                ...template,
                sequenceDigits: Number(event.currentTarget.value),
              });
            }}
          >
            {[1, 2, 3, 4, 5, 6].map((digits) => (
              <option key={digits} value={digits}>
                {digits}
              </option>
            ))}
          </select>
        </div>
      )}
    </>
  );
}

/** Champ d'un élément du modèle. */
function PartEditor({
  part,
  onChange,
}: {
  part: RenamePart;
  onChange: (part: RenamePart) => void;
}) {
  const { t } = useTranslation();
  switch (part.kind) {
    case "text":
      return (
        <input
          type="text"
          aria-label={t("rename.part.text")}
          value={part.text}
          onChange={(event) => {
            onChange({ ...part, text: event.currentTarget.value });
          }}
        />
      );
    case "captureDate":
      return (
        <select
          aria-label={t("rename.part.captureDate")}
          value={part.format}
          onChange={(event) => {
            onChange({
              ...part,
              format: event.currentTarget.value as RenameDateFormat,
            });
          }}
        >
          {DATE_FORMATS.map((format) => (
            <option key={format} value={format}>
              {t(`rename.dateFormat.${format}`)}
            </option>
          ))}
        </select>
      );
    default:
      return <span>{t(`rename.part.${part.kind}`)}</span>;
  }
}
