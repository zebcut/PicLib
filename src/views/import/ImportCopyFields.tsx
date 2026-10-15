import { useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import styles from "../../components/jobs/JobDialogs.module.css";
import { chooseBackupDestination } from "../../services/importDestination";
import type { RenamePart } from "../../services/mediaMove";
import {
  RENAME_PART_KINDS,
  RenameTemplateEditor,
} from "../library/RenameTemplateEditor";
import screenStyles from "./ImportView.module.css";
import type { ImportSettings } from "./useImportSettings";

/** Éléments du renommage à l'import : le nom actuel y est le nom d'origine. */
const IMPORT_PART_KINDS: readonly RenamePart["kind"][] =
  RENAME_PART_KINDS.filter((kind) => kind !== "currentName");

/**
 * Renommer les copies à l'import, avec les mêmes modèles que le
 * renommage des photos : l'autre fichier d'une paire et les fichiers
 * annexes gardent le même nom.
 */
export function ImportRenameFields({
  settings,
  onChange,
}: {
  settings: ImportSettings;
  onChange: (changes: Partial<ImportSettings>) => void;
}) {
  const { t } = useTranslation();
  return (
    <div className={screenStyles.group}>
      <label className={styles.option}>
        <input
          type="checkbox"
          checked={settings.useRename}
          onChange={(event) => {
            onChange({ useRename: event.target.checked });
          }}
        />
        {t("importScreen.rename.use")}
      </label>
      {settings.useRename && (
        <RenameTemplateEditor
          template={settings.renameTemplate}
          kinds={IMPORT_PART_KINDS}
          onChange={(renameTemplate) => {
            onChange({ renameTemplate });
          }}
        />
      )}
    </div>
  );
}

/**
 * Copie de sauvegarde : les originaux, intacts, copiés aussi dans
 * un second emplacement, hors du catalogue.
 */
export function ImportBackupFields({
  settings,
  onChange,
}: {
  settings: ImportSettings;
  onChange: (changes: Partial<ImportSettings>) => void;
}) {
  const { t } = useTranslation();
  const [chooseFailed, setChooseFailed] = useState(false);

  const choose = (enable: boolean) => {
    setChooseFailed(false);
    chooseBackupDestination(t("importScreen.backup.dialogTitle")).then(
      (shown) => {
        if (shown !== null) {
          onChange({
            useBackup: true,
            backupChoice: "chosen",
            shownBackup: shown,
          });
        } else if (enable) {
          onChange({ useBackup: false });
        }
      },
      (error: unknown) => {
        console.error("Failed to choose the backup folder", error);
        setChooseFailed(true);
      },
    );
  };

  return (
    <div className={screenStyles.group}>
      <label className={styles.option}>
        <input
          type="checkbox"
          checked={settings.useBackup}
          onChange={(event) => {
            const { checked } = event.target;
            onChange({ useBackup: checked });
            // Sans dossier encore choisi : il est demandé tout de suite.
            if (checked && settings.shownBackup === "") choose(true);
          }}
        />
        {t("importScreen.backup.use")}
      </label>
      {settings.useBackup && (
        <div className={styles.destination}>
          <span className={styles.path}>
            {settings.shownBackup === ""
              ? t("importScreen.backup.none")
              : settings.shownBackup}
          </span>
          <Button
            onClick={() => {
              choose(false);
            }}
          >
            {t("folderAnalysis.chooseDestination")}
          </Button>
        </div>
      )}
      {settings.useBackup && (
        <p className={styles.secondary}>
          {t("importScreen.backup.explanation")}
        </p>
      )}
      {chooseFailed && (
        <p className={styles.warning}>
          {t("folderAnalysis.destinationUnsupported")}
        </p>
      )}
    </div>
  );
}
