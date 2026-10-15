import { useEffect, useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { counted, formatBytes, formatDay } from "../i18n/format";
import { fetchFollowedPhotos, stopFollowing } from "../services/editorWatch";
import type { FollowedPhotosDto } from "../services/editorWatch";
import { fetchPreferences, savePreferences } from "../services/preferences";
import type {
  BackupFrequencyDto,
  CatalogBackupView,
  PreferencesFailure,
  PreferencesInput,
  PreviewCacheView,
} from "../services/preferences";
import { useBackendData } from "../state/useBackendData";
import type { ExternalEditors } from "../state/useExternalEditors";
import { Button } from "./Button";
import { Modal } from "./Modal";
import styles from "./PreferencesDialog.module.css";

/** Octets dans un mégaoctet (multiples de 1000, comme le Finder). */
const BYTES_PER_MB = 1_000_000;

/** Fréquences de sauvegarde proposées, dans l'ordre du menu. */
const BACKUP_FREQUENCIES: readonly BackupFrequencyDto[] = [
  "everyClose",
  "daily",
  "weekly",
];

/**
 * Fenêtre Préférences (PicLib → Préférences…) : cache des aperçus de la
 * Loupe, sauvegarde automatique du catalogue, filtre mémorisé pour chaque
 * dossier ou mot-clé, note d'une pile repliée, fichiers XMP et éditeurs
 * externes (ces derniers sont enregistrés aussitôt choisis).
 */
export function PreferencesDialog({
  onClose,
  externalEditors,
}: {
  onClose: () => void;
  /** Éditeurs externes ; absent : section non affichée. */
  externalEditors?: ExternalEditors;
}) {
  const { t } = useTranslation();
  const preferences = useBackendData(fetchPreferences, "preferences");
  // Seules les valeurs modifiées ; les autres restent celles enregistrées.
  const [changes, setChanges] = useState<Partial<PreferencesInput>>({});
  const [failure, setFailure] = useState<
    PreferencesFailure["kind"] | "coreUnreachable" | null
  >(null);
  const [isSaving, setIsSaving] = useState(false);

  const view = preferences.status === "ready" ? preferences.data : null;
  const input: PreferencesInput | null =
    view === null
      ? null
      : {
          previewCacheLimitMb: view.previewCache.limitMb,
          backupFrequency: view.catalogBackup.frequency,
          backupsToKeep: view.catalogBackup.keep,
          rememberFilterPerSource: view.rememberFilterPerSource,
          triageWholeStack: view.triageWholeStack,
          openImportOnCard: view.openImportOnCard,
          writeXmpAutomatically: view.writeXmpAutomatically,
          ...changes,
        };
  const change = (next: Partial<PreferencesInput>) => {
    setChanges((previous) => ({ ...previous, ...next }));
    setFailure(null);
  };

  const save = () => {
    if (input === null) return;
    setIsSaving(true);
    savePreferences(input).then(
      (refusal) => {
        setIsSaving(false);
        if (refusal === null) onClose();
        else setFailure(refusal.kind);
      },
      (error: unknown) => {
        console.error("Failed to save the preferences", error);
        setIsSaving(false);
        setFailure("coreUnreachable");
      },
    );
  };

  return (
    <Modal
      title={t("preferences.title")}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={input === null || isSaving}
            onClick={save}
          >
            {t("preferences.save")}
          </Button>
        </>
      }
    >
      {preferences.status === "error" && (
        <p role="alert" className={styles.error}>
          {t("preferences.loadFailed")}
        </p>
      )}
      {view !== null && input !== null && (
        <>
          <PreviewCacheSection
            cache={view.previewCache}
            limitMb={input.previewCacheLimitMb}
            onChange={(previewCacheLimitMb) => {
              change({ previewCacheLimitMb });
            }}
          />
          <CatalogBackupSection
            backup={view.catalogBackup}
            frequency={input.backupFrequency}
            keep={input.backupsToKeep}
            onChange={change}
          />
          <section className={styles.section}>
            <span className={styles.label}>{t("preferences.library")}</span>
            <label className={styles.checkbox}>
              <input
                type="checkbox"
                checked={input.rememberFilterPerSource}
                onChange={(event) => {
                  change({
                    rememberFilterPerSource: event.currentTarget.checked,
                  });
                }}
              />
              {t("preferences.rememberFilter")}
            </label>
            <p className={styles.help}>{t("preferences.rememberFilterHelp")}</p>
            <label className={styles.checkbox}>
              <input
                type="checkbox"
                checked={input.triageWholeStack}
                onChange={(event) => {
                  change({ triageWholeStack: event.currentTarget.checked });
                }}
              />
              {t("preferences.triageWholeStack")}
            </label>
            <p className={styles.help}>
              {t("preferences.triageWholeStackHelp")}
            </p>
          </section>
          <section className={styles.section}>
            <span className={styles.label}>{t("preferences.import")}</span>
            <label className={styles.checkbox}>
              <input
                type="checkbox"
                checked={input.openImportOnCard}
                onChange={(event) => {
                  change({ openImportOnCard: event.currentTarget.checked });
                }}
              />
              {t("preferences.openImportOnCard")}
            </label>
          </section>
          <section className={styles.section}>
            <span className={styles.label}>{t("preferences.xmp")}</span>
            <label className={styles.checkbox}>
              <input
                type="checkbox"
                checked={input.writeXmpAutomatically}
                onChange={(event) => {
                  change({
                    writeXmpAutomatically: event.currentTarget.checked,
                  });
                }}
              />
              {t("preferences.writeXmpAutomatically")}
            </label>
            <p className={styles.help}>
              {t("preferences.writeXmpAutomaticallyHelp")}
            </p>
          </section>
          {externalEditors !== undefined && (
            <ExternalEditorsSection editors={externalEditors} />
          )}
        </>
      )}
      {failure !== null && (
        <p role="alert" className={styles.error}>
          {failure === "invalidValue"
            ? t("preferences.invalidValue")
            : failure === "saveFailed"
              ? t("preferences.saveFailed")
              : t("preferences.loadFailed")}
        </p>
      )}
    </Modal>
  );
}

function PreviewCacheSection({
  cache,
  limitMb,
  onChange,
}: {
  cache: PreviewCacheView;
  limitMb: number;
  onChange: (limitMb: number) => void;
}) {
  const { t } = useTranslation();
  const selectId = useId();

  return (
    <section className={styles.section}>
      <label htmlFor={selectId} className={styles.label}>
        {t("preferences.previewCache")}
      </label>
      <select
        id={selectId}
        className={styles.select}
        value={limitMb}
        onChange={(event) => {
          onChange(Number(event.currentTarget.value));
        }}
      >
        {cache.limitChoicesMb.map((choice) => (
          <option key={choice} value={choice}>
            {formatBytes(choice * BYTES_PER_MB)}
          </option>
        ))}
      </select>
      {cache.usedMb !== null && (
        <p className={styles.help}>
          {t("preferences.previewCacheUsed", {
            size: formatBytes(cache.usedMb * BYTES_PER_MB),
          })}
        </p>
      )}
      <p className={styles.help}>{t("preferences.previewCacheHelp")}</p>
    </section>
  );
}

function CatalogBackupSection({
  backup,
  frequency,
  keep,
  onChange,
}: {
  backup: CatalogBackupView;
  frequency: BackupFrequencyDto;
  keep: number;
  onChange: (change: Partial<PreferencesInput>) => void;
}) {
  const { t } = useTranslation();
  const frequencyId = useId();
  const keepId = useId();

  return (
    <section className={styles.section}>
      <label htmlFor={frequencyId} className={styles.label}>
        {t("preferences.backupFrequency")}
      </label>
      <select
        id={frequencyId}
        className={styles.select}
        value={frequency}
        onChange={(event) => {
          onChange({
            backupFrequency: event.currentTarget.value as BackupFrequencyDto,
          });
        }}
      >
        {BACKUP_FREQUENCIES.map((choice) => (
          <option key={choice} value={choice}>
            {t(`preferences.frequency.${choice}`)}
          </option>
        ))}
      </select>
      <label htmlFor={keepId} className={styles.label}>
        {t("preferences.backupsToKeep")}
      </label>
      <select
        id={keepId}
        className={styles.select}
        value={keep}
        onChange={(event) => {
          onChange({ backupsToKeep: Number(event.currentTarget.value) });
        }}
      >
        {backup.keepChoices.map((choice) => (
          <option key={choice} value={choice}>
            {choice}
          </option>
        ))}
      </select>
      <p className={styles.help}>
        {backup.latest === null
          ? t("preferences.noBackupYet")
          : t("preferences.latestBackup", { date: formatDay(backup.latest) })}
      </p>
      <p className={styles.help}>{t("preferences.backupHelp")}</p>
    </section>
  );
}

/**
 * Éditeurs externes : ceux trouvés dans Applications et les
 * applications ajoutées ; le bouton radio choisit celui de Cmd+E.
 */
function ExternalEditorsSection({ editors }: { editors: ExternalEditors }) {
  const { t } = useTranslation();
  const name = useId();
  const { state, add, remove, setDefault } = editors;
  const [followed, setFollowed] = useState<FollowedPhotosDto | null>(null);
  useEffect(() => {
    let isActive = true;
    fetchFollowedPhotos().then(
      (found) => {
        if (isActive) setFollowed(found);
      },
      (error: unknown) => {
        console.error("Failed to read the followed photos", error);
      },
    );
    return () => {
      isActive = false;
    };
  }, []);
  const stop = () => {
    stopFollowing(null).then(setFollowed, (error: unknown) => {
      console.error("Failed to stop following the photos", error);
    });
  };

  return (
    <section className={styles.section}>
      <span className={styles.label}>{t("preferences.editors")}</span>
      {state !== null && state.editors.length === 0 && (
        <p className={styles.help}>{t("preferences.editorsNone")}</p>
      )}
      {state?.editors.map((editor) => (
        <div key={editor.id} className={styles.editor}>
          <label className={styles.checkbox}>
            <input
              type="radio"
              name={name}
              checked={state.defaultId === editor.id}
              onChange={() => {
                void setDefault(editor.id);
              }}
            />
            {editor.name}
          </label>
          {editor.custom && (
            <Button
              onClick={() => {
                void remove(editor.id);
              }}
            >
              {t("preferences.editorsRemove")}
            </Button>
          )}
        </div>
      ))}
      <Button
        className={styles.add}
        onClick={() => {
          void add(t("preferences.editorsAddTitle"));
        }}
      >
        {t("preferences.editorsAdd")}
      </Button>
      <p className={styles.help}>{t("preferences.editorsHelp")}</p>
      {followed !== null && (
        <div className={styles.editor}>
          <span>
            {followed.photos === 0
              ? t("preferences.editorsFollowedNone")
              : t("preferences.editorsFollowed", {
                  ...counted(followed.photos),
                  folders: t(
                    "preferences.editorsFolders",
                    counted(followed.folders),
                  ),
                })}
          </span>
          {followed.photos > 0 && (
            <Button onClick={stop}>
              {t("preferences.editorsStopFollowing")}
            </Button>
          )}
        </div>
      )}
      <p className={styles.help}>{t("preferences.editorsFollowHelp")}</p>
    </section>
  );
}
