import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import styles from "../../components/jobs/JobDialogs.module.css";
import {
  applyImportPreset,
  deleteImportPreset,
  fetchImportPresets,
  saveImportPreset,
} from "../../services/importPresets";
import { useBackendData } from "../../state/useBackendData";
import type { PlacementChoice } from "./ImportPlacementFields";
import screenStyles from "./ImportView.module.css";
import { presetInput, settingsFromPreset } from "./useImportSettings";
import type { ImportSettings, ImportSettingsState } from "./useImportSettings";

/**
 * Préréglages d'import : reprendre d'un choix tous les réglages
 * enregistrés sous un nom, ou enregistrer ceux de l'écran.
 */
export function ImportPresetBar({
  state,
  settings,
  placement,
}: {
  state: ImportSettingsState;
  settings: ImportSettings;
  placement: PlacementChoice;
}) {
  const { t } = useTranslation();
  const [version, setVersion] = useState(0);
  const [name, setName] = useState("");
  const [failed, setFailed] = useState(false);
  const presets = useBackendData(fetchImportPresets, "import presets", version);
  const list = presets.status === "ready" ? presets.data : [];
  const typed = list.find(
    (preset) => preset.name.toLowerCase() === name.trim().toLowerCase(),
  );

  const report = (error: unknown) => {
    console.error("Import preset action failed", error);
    setFailed(true);
  };

  return (
    <div className={screenStyles.group}>
      <label className={styles.option}>
        {t("importScreen.presets.label")}
        <select
          value=""
          disabled={list.length === 0}
          onChange={(event) => {
            const chosen = list.find(
              (preset) => String(preset.id) === event.target.value,
            );
            if (chosen === undefined) return;
            setFailed(false);
            setName(chosen.name);
            applyImportPreset(chosen.id).then((preset) => {
              state.update(
                settingsFromPreset(settings, preset, state.templates ?? []),
              );
            }, report);
          }}
        >
          <option value="">{t("importScreen.presets.choose")}</option>
          {list.map((preset) => (
            <option key={preset.id} value={preset.id}>
              {preset.name}
            </option>
          ))}
        </select>
      </label>
      <div className={screenStyles.adders}>
        <input
          type="text"
          aria-label={t("importScreen.presets.name")}
          placeholder={t("importScreen.presets.name")}
          value={name}
          onChange={(event) => {
            setName(event.target.value);
          }}
        />
        <Button
          disabled={name.trim() === ""}
          onClick={() => {
            setFailed(false);
            saveImportPreset(name, presetInput(settings, placement)).then(
              () => {
                setVersion((current) => current + 1);
              },
              report,
            );
          }}
        >
          {typed === undefined
            ? t("importScreen.presets.save")
            : t("importScreen.presets.update")}
        </Button>
        <Button
          disabled={typed === undefined}
          onClick={() => {
            if (typed === undefined) return;
            setFailed(false);
            deleteImportPreset(typed.id).then(() => {
              setName("");
              setVersion((current) => current + 1);
            }, report);
          }}
        >
          {t("importScreen.presets.delete")}
        </Button>
      </div>
      {failed && (
        <p className={styles.warning}>{t("importScreen.presets.failed")}</p>
      )}
    </div>
  );
}
