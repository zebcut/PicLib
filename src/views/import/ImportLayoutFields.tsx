import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import styles from "../../components/jobs/JobDialogs.module.css";
import type { DatePart } from "../../services/importSources";
import { DATE_PRESETS, layoutChoice, layoutFor } from "./layouts";
import type { LayoutChoice } from "./layouts";
import screenStyles from "./ImportView.module.css";
import type { ImportSettings } from "./useImportSettings";

/** Éléments qu'on peut ajouter à un modèle par date, dans l'ordre des boutons. */
const PART_KINDS: readonly DatePart["kind"][] = [
  "year",
  "month",
  "monthName",
  "day",
  "text",
  "level",
];

function newPart(kind: DatePart["kind"]): DatePart {
  return kind === "text" ? { kind, text: "" } : { kind };
}

/**
 * Rangement des copies : par date (modèles prêts à l'emploi ou
 * personnalisé), dans un seul dossier, ou en conservant l'arborescence
 * d'origine ; et le sous-dossier de séance.
 */
export function ImportLayoutFields({
  settings,
  onChange,
}: {
  settings: ImportSettings;
  onChange: (changes: Partial<ImportSettings>) => void;
}) {
  const { t } = useTranslation();
  const layoutId = useId();
  const sessionId = useId();
  const { layout } = settings;
  // Un modèle personnalisé reste en édition même s'il ressemble à un
  // modèle prêt à l'emploi (celui dont il part).
  const [isCustom, setIsCustom] = useState(false);
  const choice = isCustom ? "customDate" : layoutChoice(layout);

  return (
    <div className={screenStyles.group}>
      <label className={styles.option} htmlFor={layoutId}>
        {t("importScreen.layout.label")}
      </label>
      <select
        id={layoutId}
        value={choice}
        onChange={(event) => {
          const chosen = event.target.value as LayoutChoice;
          setIsCustom(chosen === "customDate");
          onChange({ layout: layoutFor(chosen, layout) });
        }}
      >
        {DATE_PRESETS.map((preset) => (
          <option key={preset.id} value={`preset:${preset.id}`}>
            {t("importScreen.layout.byDate", { pattern: preset.label })}
          </option>
        ))}
        <option value="customDate">
          {t("importScreen.layout.customDate")}
        </option>
        <option value="singleFolder">
          {t("importScreen.layout.singleFolder")}
        </option>
        <option value="sourceTree">
          {t("importScreen.layout.sourceTree")}
        </option>
      </select>
      {layout.kind === "date" && choice === "customDate" && (
        <>
          <ol
            className={screenStyles.parts}
            aria-label={t("importScreen.layout.parts")}
          >
            {layout.parts.map((part, index) => (
              <li key={index} className={screenStyles.part}>
                {part.kind === "text" ? (
                  <input
                    type="text"
                    aria-label={t("importScreen.layout.part.text")}
                    value={part.text}
                    onChange={(event) => {
                      onChange({
                        layout: {
                          kind: "date",
                          parts: layout.parts.map((old, at) =>
                            at === index
                              ? { kind: "text", text: event.target.value }
                              : old,
                          ),
                        },
                      });
                    }}
                  />
                ) : (
                  <span>{t(`importScreen.layout.part.${part.kind}`)}</span>
                )}
                <button
                  type="button"
                  aria-label={t("importScreen.layout.removePart")}
                  onClick={() => {
                    onChange({
                      layout: {
                        kind: "date",
                        parts: layout.parts.filter((_, at) => at !== index),
                      },
                    });
                  }}
                >
                  ×
                </button>
              </li>
            ))}
          </ol>
          <div className={screenStyles.adders}>
            {PART_KINDS.map((kind) => (
              <Button
                key={kind}
                onClick={() => {
                  onChange({
                    layout: {
                      kind: "date",
                      parts: [...layout.parts, newPart(kind)],
                    },
                  });
                }}
              >
                + {t(`importScreen.layout.part.${kind}`)}
              </Button>
            ))}
          </div>
        </>
      )}
      {layout.kind === "singleFolder" && (
        <input
          type="text"
          aria-label={t("importScreen.layout.folderName")}
          placeholder={t("importScreen.layout.folderName")}
          value={layout.name}
          onChange={(event) => {
            onChange({
              layout: { kind: "singleFolder", name: event.target.value },
            });
          }}
        />
      )}
      <label className={styles.option}>
        <input
          type="checkbox"
          checked={settings.useSession}
          onChange={(event) => {
            onChange({ useSession: event.target.checked });
          }}
        />
        {t("importScreen.layout.useSession")}
      </label>
      {settings.useSession && (
        <input
          id={sessionId}
          type="text"
          aria-label={t("importScreen.layout.session")}
          placeholder={t("importScreen.layout.sessionPlaceholder")}
          value={settings.session}
          onChange={(event) => {
            onChange({ session: event.target.value });
          }}
        />
      )}
    </div>
  );
}
