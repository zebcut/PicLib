import { useState } from "react";
import { useTranslation } from "react-i18next";

import styles from "../../components/jobs/JobDialogs.module.css";
import { counted } from "../../i18n/format";
import { chooseImportDestination } from "../../services/importDestination";
import { ImportLayoutFields } from "./ImportLayoutFields";
import { ImportPlacementFields } from "./ImportPlacementFields";
import { ImportPresetBar } from "./ImportPresetBar";
import { ImportTreePreview } from "./ImportTreePreview";
import type { PlacementChoice } from "./ImportPlacementFields";
import { ImportBackupFields, ImportRenameFields } from "./ImportCopyFields";
import { arrangementOf, renameOf } from "./useImportSettings";
import type { ImportSettingsState } from "./useImportSettings";
import screenStyles from "./ImportView.module.css";

/**
 * Réglages de l'écran d'import : laisser ou copier (où, et rangés comment,
 * avec l'aperçu de l'arborescence), doublons, empilement automatique,
 * modèle de métadonnées.
 */
export function ImportOptionsPanel({
  state,
  placement,
  isCameraCard,
  analysisJobId,
  selected,
  months,
}: {
  state: ImportSettingsState;
  /** Façon d'importer réellement proposée (copie forcée pour une carte). */
  placement: PlacementChoice;
  isCameraCard: boolean;
  /** Analyse dont les fichiers sont montrés (`null` : pas encore lue). */
  analysisJobId: number | null;
  /** Fichiers cochés. */
  selected: readonly number[];
  /** Noms des mois dans la langue de l'interface. */
  months: string[];
}) {
  const { t } = useTranslation();
  const { settings, update, preferences, templates } = state;
  const [chooseFailed, setChooseFailed] = useState(false);
  if (settings === null) {
    return <aside className={screenStyles.sidePanel} aria-busy="true" />;
  }

  const chooseDestination = () => {
    setChooseFailed(false);
    chooseImportDestination(t("folderAnalysis.destinationDialogTitle")).then(
      (shown) => {
        if (shown !== null) {
          update({
            placement: {
              ...placement,
              destination: "chosen",
              shownDestination: shown,
            },
          });
        }
      },
      (error: unknown) => {
        console.error("Failed to choose the destination", error);
        setChooseFailed(true);
      },
    );
  };

  return (
    <aside
      className={[screenStyles.sidePanel, screenStyles.options].join(" ")}
      aria-label={t("importScreen.options")}
    >
      <h2 className={screenStyles.heading}>{t("importScreen.options")}</h2>
      <ImportPresetBar
        state={state}
        settings={settings}
        placement={placement}
      />
      <ImportPlacementFields
        choice={placement}
        defaultDestination={preferences?.defaultDestination ?? ""}
        isCameraCard={isCameraCard}
        canCopy={preferences !== null}
        chooseFailed={chooseFailed}
        onChange={(choice) => {
          update({ placement: choice });
        }}
        onChooseDestination={chooseDestination}
      />
      {placement.mode === "copy" && (
        <>
          <ImportLayoutFields settings={settings} onChange={update} />
          <ImportRenameFields settings={settings} onChange={update} />
          <ImportBackupFields settings={settings} onChange={update} />
          {analysisJobId !== null && (
            <ImportTreePreview
              analysisJobId={analysisJobId}
              request={{
                selected: [...selected],
                destination: placement.destination,
                arrangement: arrangementOf(settings, months),
                rename: renameOf(settings),
              }}
            />
          )}
        </>
      )}
      <label className={styles.option}>
        <input
          type="checkbox"
          checked={settings.skipDuplicates}
          onChange={(event) => {
            update({ skipDuplicates: event.target.checked });
          }}
        />
        {t("folderAnalysis.skipDuplicates")}
      </label>
      <label className={styles.option}>
        <input
          type="checkbox"
          checked={settings.stackPairs}
          onChange={(event) => {
            update({ stackPairs: event.target.checked });
          }}
        />
        {t("folderAnalysis.stackPairs")}
      </label>
      <div className={styles.option}>
        <label className={styles.option}>
          <input
            type="checkbox"
            checked={settings.stackBursts}
            onChange={(event) => {
              update({ stackBursts: event.target.checked });
            }}
          />
          {t("folderAnalysis.stackBursts")}
        </label>
        <select
          aria-label={t("folderAnalysis.burstGap")}
          value={settings.burstGapSeconds}
          disabled={!settings.stackBursts}
          onChange={(event) => {
            update({ burstGapSeconds: Number(event.target.value) });
          }}
        >
          {(preferences?.burstGapChoices ?? [settings.burstGapSeconds]).map(
            (choice) => (
              <option key={choice} value={choice}>
                {t("stacks.auto.seconds", counted(choice))}
              </option>
            ),
          )}
        </select>
        {t("folderAnalysis.burstGapAfter")}
      </div>
      <label className={styles.option}>
        {t("folderAnalysis.template")}
        <select
          value={settings.templateId ?? ""}
          disabled={templates === null}
          onChange={(event) => {
            const { value } = event.target;
            update({ templateId: value === "" ? null : Number(value) });
          }}
        >
          <option value="">{t("folderAnalysis.noTemplate")}</option>
          {templates?.map((template) => (
            <option key={template.id} value={template.id}>
              {template.name}
            </option>
          ))}
        </select>
      </label>
      {settings.templateId !== null && (
        <p className={styles.secondary}>{t("folderAnalysis.templateHint")}</p>
      )}
    </aside>
  );
}
