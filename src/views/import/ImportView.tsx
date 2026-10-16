import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import jobStyles from "../../components/jobs/JobDialogs.module.css";
import { UnreadableLines } from "../../components/jobs/Lines";
import { counted, formatBytes, progressCounts } from "../../i18n/format";
import {
  chooseImportSource,
  forgetImportSource,
  listImportSources,
} from "../../services/importSources";
import type {
  ImportFileDto,
  ImportSourceDto,
  ImportSourcesDto,
} from "../../services/importSources";
import type { JobUpdate } from "../../services/jobs";
import type { FolderImport } from "../../state/useImport";
import { ImportGrid } from "./ImportGrid";
import { ImportOptionsPanel } from "./ImportOptionsPanel";
import { ImportSourcePanel } from "./ImportSourcePanel";
import { analysisFailureMessage, importStartMessage } from "./importMessages";
import {
  checkedFiles,
  groupMembers,
  initialSelection,
  isCheckable,
  selectAll,
  toggleFile,
} from "./importSelection";
import styles from "./ImportView.module.css";
import { monthNames } from "./layouts";
import { useImportAnalysis } from "./useImportAnalysis";
import {
  effectivePlacement,
  importChoices,
  useImportSettings,
} from "./useImportSettings";

/** Intervalle entre deux relectures des sources (cartes branchées). */
const SOURCES_REFRESH_MS = 3000;

/**
 * Écran d'import, à la place de la bibliothèque :
 * la source à gauche, ses fichiers au centre avec une case à cocher
 * chacun, les réglages à droite. Seuls les fichiers cochés sont importés ;
 * l'import se poursuit ensuite en tâche de fond. Échap ferme l'écran.
 *
 * @param initialSourceId source à analyser dès l'ouverture ; sans elle, la
 *   première carte mémoire branchée.
 * @param onlyNames fichiers glissés du Finder, à la racine de cette source :
 *   seuls ceux-là sont cochés d'emblée.
 */
export function ImportView({
  jobs,
  onDismissJob,
  folderImport,
  initialSourceId,
  onlyNames,
  onClose,
}: {
  jobs: readonly JobUpdate[];
  onDismissJob: (jobId: number) => void;
  folderImport: FolderImport;
  initialSourceId?: number;
  onlyNames?: readonly string[];
  onClose: () => void;
}) {
  const { t, i18n } = useTranslation();
  const months = useMemo(() => monthNames(i18n.language), [i18n.language]);
  const [sources, setSources] = useState<ImportSourcesDto | null>(null);
  const [sourceId, setSourceId] = useState<number | null>(
    initialSourceId ?? null,
  );
  const [chooseFailed, setChooseFailed] = useState(false);

  // Les sources sont relues régulièrement : une carte branchée apparaît.
  const [sourcesVersion, setSourcesVersion] = useState(0);
  const refreshSources = useCallback(() => {
    setSourcesVersion((version) => version + 1);
  }, []);
  useEffect(() => {
    let isActive = true;
    const read = () => {
      listImportSources().then(
        (list) => {
          if (isActive) setSources(list);
        },
        (error: unknown) => {
          console.error("Failed to list the import sources", error);
          if (isActive) {
            setSources((current) => current ?? { cards: [], folders: [] });
          }
        },
      );
    };
    read();
    const timer = window.setInterval(read, SOURCES_REFRESH_MS);
    return () => {
      isActive = false;
      window.clearInterval(timer);
    };
  }, [sourcesVersion]);

  // Sans source choisie, la première carte mémoire, une seule fois.
  const [hasPicked, setHasPicked] = useState(initialSourceId !== undefined);
  if (!hasPicked && sources !== null) {
    setHasPicked(true);
    const card = sources.cards[0];
    if (card !== undefined) setSourceId(card.id);
  }

  const chooseFolder = () => {
    setChooseFailed(false);
    chooseImportSource(t("folderAnalysis.dialogTitle")).then(
      (result) => {
        if (result.status === "error") {
          setChooseFailed(true);
          return;
        }
        const source = result.data;
        if (source === null) return;
        setHasPicked(true);
        setSourceId(source.id);
        // Il rejoint les derniers dossiers.
        refreshSources();
      },
      (error: unknown) => {
        console.error("Failed to choose a source folder", error);
        setChooseFailed(true);
      },
    );
  };

  const forgetFolder = (source: ImportSourceDto) => {
    if (source.id === sourceId) setSourceId(null);
    forgetImportSource(source.id).then(refreshSources, (error: unknown) => {
      console.error("Failed to forget the import folder", error);
    });
  };

  const settingsState = useImportSettings();
  const { settings } = settingsState;
  // L'analyse attend les réglages : ils disent s'il faut lire les
  // sous-dossiers.
  const analysis = useImportAnalysis(
    settings === null ? null : sourceId,
    settings?.includeSubfolders ?? true,
    jobs,
    onDismissJob,
  );
  const skipDuplicates = settings?.skipDuplicates ?? true;

  const files = analysis.status === "ready" ? analysis.files : null;
  // Les fichiers cochés d'emblée sont relus pour chaque nouvelle source.
  const [selection, setSelection] = useState<{
    files: readonly ImportFileDto[] | null;
    checked: ReadonlySet<number>;
  }>({ files: null, checked: new Set() });
  if (selection.files !== files) {
    setSelection({
      files,
      checked:
        files === null
          ? new Set()
          : initialSelection(
              files,
              sourceId === initialSourceId ? onlyNames : undefined,
            ),
    });
  }
  const { checked } = selection;
  const setChecked = useCallback(
    (
      change:
        | ReadonlySet<number>
        | ((current: ReadonlySet<number>) => ReadonlySet<number>),
    ) => {
      setSelection((current) => ({
        ...current,
        checked:
          typeof change === "function" ? change(current.checked) : change,
      }));
    },
    [],
  );
  const groups = useMemo(() => groupMembers(files ?? []), [files]);
  const toggle = useCallback(
    (file: ImportFileDto) => {
      if (files === null) return;
      setChecked((current) =>
        toggleFile(current, file, files, groups, skipDuplicates),
      );
    },
    [files, groups, skipDuplicates, setChecked],
  );
  const checkable = useCallback(
    (file: ImportFileDto) => isCheckable(file, skipDuplicates),
    [skipDuplicates],
  );
  const selected =
    files === null ? [] : checkedFiles(checked, files, skipDuplicates);
  const selectedSize =
    files === null
      ? 0
      : selected.reduce((total, index) => total + (files[index]?.size ?? 0), 0);

  const source =
    [...(sources?.cards ?? []), ...(sources?.folders ?? [])].find(
      (known) => known.id === sourceId,
    ) ?? null;
  const isCameraCard =
    source?.kind === "card" ||
    (analysis.status === "ready" && analysis.summary.isCameraCard);
  const placement =
    settings === null
      ? null
      : effectivePlacement(
          settings.placement,
          settingsState.preferences,
          isCameraCard,
        );
  // Une carte ne s'importe qu'en copie : impossible sans ses réglages. Une
  // copie de sauvegarde demande son dossier.
  const canCopy =
    (!isCameraCard || settingsState.preferences !== null) &&
    !(
      placement?.mode === "copy" &&
      settings?.useBackup === true &&
      settings.shownBackup === ""
    );

  const [isStarting, setIsStarting] = useState(false);
  const startImport = () => {
    if (
      analysis.status !== "ready" ||
      settings === null ||
      placement === null
    ) {
      return;
    }
    setIsStarting(true);
    void folderImport
      .start(
        analysis.jobId,
        importChoices(settings, placement, selected, months),
      )
      .then((started) => {
        setIsStarting(false);
        if (started) onClose();
      });
  };

  // Échap ferme l'écran, sauf si une fenêtre est ouverte par-dessus.
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "Escape") return;
      if (document.querySelectorAll('[aria-modal="true"]').length > 1) return;
      event.preventDefault();
      onClose();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [onClose]);

  return (
    <div
      className={styles.screen}
      role="dialog"
      aria-modal="true"
      aria-label={t("importScreen.title")}
    >
      <ImportSourcePanel
        cards={sources?.cards ?? []}
        folders={sources?.folders ?? []}
        selectedId={sourceId}
        includeSubfolders={settings?.includeSubfolders ?? true}
        onSelect={(chosen) => {
          setHasPicked(true);
          setSourceId(chosen.id);
        }}
        onForget={forgetFolder}
        onIncludeSubfolders={(includeSubfolders) => {
          settingsState.update({ includeSubfolders });
        }}
        onChooseFolder={chooseFolder}
      />
      <main className={styles.center}>
        <header className={styles.toolbar}>
          <h1 className={styles.title}>
            {source === null
              ? t("importScreen.title")
              : t("importScreen.titleWithSource", { source: source.name })}
          </h1>
          {files !== null && files.length > 0 && (
            <>
              <span className={styles.count}>
                {t("importScreen.checkedCount", {
                  ...counted(selected.length),
                  total: files.length,
                  size: formatBytes(selectedSize),
                })}
              </span>
              <Button
                onClick={() => {
                  setChecked(selectAll(files, skipDuplicates));
                }}
              >
                {t("importScreen.checkAll")}
              </Button>
              <Button
                onClick={() => {
                  setChecked(new Set());
                }}
              >
                {t("importScreen.uncheckAll")}
              </Button>
            </>
          )}
        </header>
        {chooseFailed && (
          <p className={[jobStyles.warning, styles.notice].join(" ")}>
            {t("folderAnalysis.unsupportedLocation")}
          </p>
        )}
        <AnalysisContent
          analysis={analysis}
          checked={checked}
          isCheckable={checkable}
          onToggle={toggle}
        />
        <footer className={styles.footer}>
          {analysis.status === "ready" && (
            <UnreadableLines
              count={analysis.summary.unreadableCount}
              examples={analysis.summary.unreadableExamples}
              messageKey="folderAnalysis.unreadable"
            />
          )}
          <span className={styles.spacer} />
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={
              selected.length === 0 ||
              placement === null ||
              !canCopy ||
              isStarting
            }
            onClick={startImport}
          >
            {placement?.mode === "copy"
              ? t("importScreen.copy", counted(selected.length))
              : t("importScreen.add", counted(selected.length))}
          </Button>
        </footer>
      </main>
      {placement !== null && (
        <ImportOptionsPanel
          state={settingsState}
          placement={placement}
          isCameraCard={isCameraCard}
          analysisJobId={analysis.status === "ready" ? analysis.jobId : null}
          selected={selected}
          months={months}
        />
      )}
      {folderImport.startFailure !== null && (
        <Modal
          title={t("importResult.failedTitle")}
          actions={
            <Button
              variant="primary"
              autoFocus
              onClick={folderImport.dismissStartFailure}
            >
              {t("common.ok")}
            </Button>
          }
        >
          <p className={jobStyles.text}>
            {importStartMessage(t, folderImport.startFailure)}
          </p>
        </Modal>
      )}
    </div>
  );
}

/** Centre de l'écran : invitation, progression, échec ou grille. */
function AnalysisContent({
  analysis,
  checked,
  isCheckable,
  onToggle,
}: {
  analysis: ReturnType<typeof useImportAnalysis>;
  checked: ReadonlySet<number>;
  isCheckable: (file: ImportFileDto) => boolean;
  onToggle: (file: ImportFileDto) => void;
}) {
  const { t } = useTranslation();
  switch (analysis.status) {
    case "none":
      return (
        <p className={styles.placeholder}>{t("importScreen.pickSource")}</p>
      );
    case "starting":
      return <p className={styles.placeholder}>{t("importScreen.reading")}</p>;
    case "running":
      return (
        <p className={styles.placeholder} role="status">
          {analysis.total === null
            ? t("jobs.filesFound", counted(analysis.progress))
            : t(
                "importScreen.readingDates",
                progressCounts(analysis.progress, analysis.total),
              )}
        </p>
      );
    case "failed":
      return (
        <p className={[styles.placeholder, jobStyles.warning].join(" ")}>
          {analysisFailureMessage(t, analysis)}
        </p>
      );
    case "ready":
      return analysis.files.length === 0 ? (
        <p className={styles.placeholder}>{t("folderAnalysis.nothingFound")}</p>
      ) : (
        <ImportGrid
          analysisJobId={analysis.jobId}
          files={analysis.files}
          checked={checked}
          isCheckable={isCheckable}
          onToggle={onToggle}
        />
      );
  }
}
