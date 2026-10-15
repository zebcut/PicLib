import { useCallback, useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { CatalogSwitchDialog } from "./components/CatalogSwitchDialog";
import { EditNoticeToast } from "./components/EditNoticeToast";
import { FinishedJobDialog } from "./components/jobs/FinishedJobDialog";
import { PreferencesDialog } from "./components/PreferencesDialog";
import { JobIndicator } from "./components/jobs/JobIndicator";
import { StartFailureDialog } from "./components/jobs/StartFailureDialog";
import { installAppMenu } from "./menu/appMenu";
import { fetchCatalogOverview } from "./services/catalogInfo";
import { subscribeToImportRequests } from "./services/importRequests";
import { fetchStartupStatus } from "./services/startup";
import { saveMetadataToFiles as saveMetadataToFilesIn } from "./services/xmp";
import { describeSummary } from "./i18n/triage";
import { counted } from "./i18n/format";
import type { EditNotice } from "./state/useCatalogEdits";
import { useShortcuts, useShortcutTrigger } from "./shortcuts/ShortcutProvider";
import type { StartupStatus } from "./services/startup";
import { useBackendData } from "./state/useBackendData";
import type { BackendData } from "./state/useBackendData";
import { useCatalogEdits } from "./state/useCatalogEdits";
import { requestExternalEdit } from "./state/externalEditRequests";
import { useExternalEditors } from "./state/useExternalEditors";
import { useKeywordTransfer } from "./state/useKeywordTransfer";
import { useCatalogSwitch } from "./state/useCatalogSwitch";
import type { CatalogSwitch } from "./state/useCatalogSwitch";
import { useFileMetadataReading } from "./state/useFileMetadataReading";
import { useImport } from "./state/useImport";
import { useJobs } from "./state/useJobs";
import { useStoredBoolean } from "./state/useStoredBoolean";
import { useThrottledValue } from "./state/useThrottledValue";
import { ImportView } from "./views/import/ImportView";
import { KeywordTransferDialog } from "./views/library/KeywordTransferDialog";
import type { EditActions } from "./views/library/LibraryBrowser";
import { LibraryView } from "./views/library/LibraryView";
import { StartupErrorView } from "./views/startup-error/StartupErrorView";
import { WelcomeView } from "./views/welcome/WelcomeView";

/**
 * Intervalle minimal entre deux relectures de la grille pendant la
 * fabrication des vignettes : assez court pour les voir apparaître, assez
 * long pour ne pas redessiner la grille dix fois par seconde.
 */
const THUMBNAIL_REFRESH_MS = 1000;

/** Clé de l'avance automatique retenue d'une session à l'autre. */
const AUTO_ADVANCE_STORAGE_KEY = "piclib.autoAdvance";

/** Clé de l'avance automatique du mode Tri, activée par défaut. */
const CULL_AUTO_ADVANCE_STORAGE_KEY = "piclib.cull.autoAdvance";

/**
 * Composant racine (dans le `ShortcutProvider` de `main.tsx`) : installe la
 * barre de menus, suit les tâches de fond et les modifications du catalogue
 * (Cmd+Z), puis affiche la bibliothèque (ou l'accueil si le catalogue est
 * vide) quand le catalogue est prêt, ou un écran expliquant pourquoi il n'a
 * pas pu être ouvert.
 */
export function App() {
  const { t } = useTranslation();
  const startup = useBackendData(fetchStartupStatus, "startup status");
  const catalogSwitch = useCatalogSwitch();
  const jobs = useJobs();
  const folderImport = useImport();
  // Écran d'import ouvert, avec la source à analyser d'emblée s'il y en a
  // une (sinon la première carte mémoire) ; une nouvelle demande (carte
  // branchée, dossier glissé du Finder) le rouvre sur sa source.
  const [importScreen, setImportScreen] = useState<{
    key: number;
    sourceId?: number;
    onlyNames?: string[];
  } | null>(null);
  const openImportScreen = useCallback(() => {
    setImportScreen((current) => current ?? { key: 0 });
  }, []);
  useEffect(() => {
    let isActive = true;
    subscribeToImportRequests((request) => {
      if (!isActive) return;
      setImportScreen((current) => ({
        key: (current?.key ?? 0) + 1,
        sourceId: request.sourceId,
        onlyNames: request.onlyNames,
      }));
    }).catch((error: unknown) => {
      console.error("Failed to subscribe to import requests", error);
    });
    return () => {
      isActive = false;
    };
  }, []);
  const closeImportScreen = useCallback(() => {
    setImportScreen(null);
  }, []);
  const [isPreferencesOpen, setIsPreferencesOpen] = useState(false);
  const externalEditors = useExternalEditors();
  const reloadEditors = externalEditors.reload;
  const openPreferences = useCallback(() => {
    // Une application a pu être installée depuis la dernière lecture.
    reloadEditors();
    setIsPreferencesOpen(true);
  }, [reloadEditors]);
  const trigger = useShortcutTrigger();
  const edits = useCatalogEdits();
  const [autoAdvance, setAutoAdvance] = useStoredBoolean(
    AUTO_ADVANCE_STORAGE_KEY,
    false,
  );
  const toggleAutoAdvance = useCallback(() => {
    setAutoAdvance(!autoAdvance);
  }, [autoAdvance, setAutoAdvance]);
  const [cullAutoAdvance, setCullAutoAdvance] = useStoredBoolean(
    CULL_AUTO_ADVANCE_STORAGE_KEY,
    true,
  );
  const toggleCullAutoAdvance = useCallback(() => {
    setCullAutoAdvance(!cullAutoAdvance);
  }, [cullAutoAdvance, setCullAutoAdvance]);
  const {
    undo,
    redo,
    applyTriage,
    rotate,
    removeRejected,
    removePhotos,
    removeFolder,
    trashFolder,
    moveFolder,
    setMetadata,
    changeCaptureTime,
    setLocation,
    removeLocation,
    readSidecarMetadata,
    addKeywords,
    removeKeyword,
    addKeyword,
    changeKeywordList,
    importKeywords,
    changeFilterPreset,
    changeCollections,
    changeStacks,
    relocateMedia,
    relocateFolder,
    moveMedia,
    moveMediaToNewFolder,
    renameFolder,
    renameMedia,
    applyTemplate,
    applyFileMetadata,
    editInExternalEditor,
  } = edits;
  const fileMetadata = useFileMetadataReading(
    jobs.jobs,
    applyFileMetadata,
    jobs.dismiss,
  );
  const readFileMetadata = fileMetadata.start;
  const saveMetadataToFiles = useCallback(
    (mediaIds: number[], overwriteChanged?: boolean) => {
      if (mediaIds.length === 0) return;
      saveMetadataToFilesIn(mediaIds, overwriteChanged).then(
        (started) => {
          if (started.status === "error") {
            console.error("XMP writing not started", started.error);
          }
        },
        (error: unknown) => {
          console.error("Failed to start writing the XMP files", error);
        },
      );
    },
    [],
  );
  useShortcuts({ "edit.undo": undo, "edit.redo": redo });
  const keywordTransfer = useKeywordTransfer(importKeywords);
  const thumbnailUpdates = useThrottledValue(
    jobs.thumbnailUpdates,
    THUMBNAIL_REFRESH_MS,
  );

  const { openCatalog, createCatalog } = catalogSwitch;
  const isCatalogReady =
    startup.status === "ready" && startup.data.status === "ready";
  const importPhotos = isCatalogReady ? openImportScreen : undefined;
  const importKeywordList = isCatalogReady
    ? keywordTransfer.startImport
    : undefined;
  const exportKeywordList = isCatalogReady
    ? keywordTransfer.startExport
    : undefined;
  const { undo: nextUndo, redo: nextRedo } = edits.history;
  const undoLabel =
    nextUndo === null
      ? null
      : t("menu.undoWith", { description: describeSummary(t, nextUndo) });
  const redoLabel =
    nextRedo === null
      ? null
      : t("menu.redoWith", { description: describeSummary(t, nextRedo) });

  // Photos traitées revenues d'un éditeur.
  const added = jobs.editedVersionsAdded;
  const versionsNotice: EditNotice | null = useMemo(
    () =>
      added === null || added.added === 0
        ? null
        : {
            kind: "info",
            message: t("externalEdit.added", counted(added.added)),
            key: added.key,
          },
    [added, t],
  );
  const editors = externalEditors.state;
  const menuEditors = useMemo(() => editors?.editors ?? [], [editors]);
  const defaultEditorName =
    menuEditors.find((editor) => editor.id === editors?.defaultId)?.name ??
    null;

  useEffect(() => {
    installAppMenu(t, {
      openCatalog,
      createCatalog,
      openPreferences,
      importPhotos,
      importKeywords: importKeywordList,
      exportKeywords: exportKeywordList,
      trigger,
      undoLabel,
      redoLabel,
      photo: {
        enabled: isCatalogReady,
        autoAdvance,
        toggleAutoAdvance,
        cullAutoAdvance,
        toggleCullAutoAdvance,
        editors: menuEditors,
        defaultEditorName,
        editIn: requestExternalEdit,
      },
    }).catch((error: unknown) => {
      console.error("Failed to install the application menu", error);
    });
  }, [
    t,
    openCatalog,
    createCatalog,
    openPreferences,
    importPhotos,
    importKeywordList,
    exportKeywordList,
    trigger,
    undoLabel,
    redoLabel,
    isCatalogReady,
    autoAdvance,
    toggleAutoAdvance,
    cullAutoAdvance,
    toggleCullAutoAdvance,
    menuEditors,
    defaultEditorName,
  ]);

  const triage: EditActions = useMemo(
    () => ({
      apply: applyTriage,
      rotate,
      removeRejected,
      removePhotos,
      removeFolder,
      trashFolder,
      moveFolder,
      setMetadata,
      changeCaptureTime,
      setLocation,
      removeLocation,
      readSidecarMetadata,
      addKeywords,
      removeKeyword,
      addKeyword,
      changeKeywordList,
      importKeywordList,
      exportKeywordList,
      changeFilterPreset,
      changeCollections,
      changeStacks,
      relocateMedia,
      relocateFolder,
      moveMedia,
      moveMediaToNewFolder,
      renameFolder,
      renameMedia,
      applyTemplate,
      readFileMetadata,
      saveMetadataToFiles,
      externalEditors: editors,
      editInExternalEditor,
      openPreferences,
      autoAdvance,
      cullAutoAdvance,
    }),
    [
      editors,
      editInExternalEditor,
      openPreferences,
      changeFilterPreset,
      changeCollections,
      changeStacks,
      relocateMedia,
      relocateFolder,
      moveMedia,
      moveMediaToNewFolder,
      renameFolder,
      renameMedia,
      applyTriage,
      rotate,
      removeRejected,
      removePhotos,
      removeFolder,
      trashFolder,
      moveFolder,
      setMetadata,
      changeCaptureTime,
      setLocation,
      removeLocation,
      readSidecarMetadata,
      addKeywords,
      removeKeyword,
      addKeyword,
      changeKeywordList,
      importKeywordList,
      exportKeywordList,
      applyTemplate,
      readFileMetadata,
      saveMetadataToFiles,
      autoAdvance,
      cullAutoAdvance,
    ],
  );

  return (
    <>
      <Screen
        startup={startup}
        catalogSwitch={catalogSwitch}
        onImport={openImportScreen}
        importKey={jobs.finishedImports}
        thumbnailsKey={thumbnailUpdates}
        editsKey={edits.version}
        trashKey={jobs.finishedTrash}
        placesKey={
          jobs.finishedPlaceLookups +
          jobs.finishedMissingChecks +
          jobs.finishedXmpWrites +
          (jobs.editedVersionsAdded?.key ?? 0)
        }
        triage={triage}
      />
      <EditNoticeToast notice={edits.notice} />
      <EditNoticeToast notice={versionsNotice} />
      <JobIndicator jobs={jobs.jobs} />
      {importScreen !== null && isCatalogReady && (
        <ImportView
          key={importScreen.key}
          jobs={jobs.jobs}
          onDismissJob={jobs.dismiss}
          folderImport={folderImport}
          initialSourceId={importScreen.sourceId}
          onlyNames={importScreen.onlyNames}
          onClose={closeImportScreen}
        />
      )}
      <FinishedJobDialog
        jobs={jobs.jobs}
        onDismiss={jobs.dismiss}
        onApplyFileMetadata={applyFileMetadata}
        onDiscardFileMetadata={fileMetadata.discard}
      />
      <StartFailureDialog
        fileMetadataFailure={fileMetadata.startFailure}
        onDismissFileMetadata={fileMetadata.dismissStartFailure}
      />
      <CatalogSwitchDialog catalogSwitch={catalogSwitch} />
      {keywordTransfer.dialog !== null && (
        <KeywordTransferDialog
          dialog={keywordTransfer.dialog}
          onImport={keywordTransfer.confirmImport}
          onClose={keywordTransfer.close}
        />
      )}
      {isPreferencesOpen && (
        <PreferencesDialog
          externalEditors={externalEditors}
          onClose={() => {
            setIsPreferencesOpen(false);
          }}
        />
      )}
    </>
  );
}

/** Ce qui fait relire la bibliothèque, et ses actions. */
interface LibraryInputs {
  onImport: () => void;
  /** Change à chaque import terminé. */
  importKey: number;
  /** Change quand de nouvelles vignettes sont prêtes. */
  thumbnailsKey: number;
  /** Change après chaque modification du catalogue. */
  editsKey: number;
  /** Change à chaque mise à la corbeille terminée. */
  trashKey: number;
  /** Change quand le lieu de photos a été rempli depuis leur GPS. */
  placesKey: number;
  triage: EditActions;
}

function Screen({
  startup,
  catalogSwitch,
  ...library
}: LibraryInputs & {
  startup: BackendData<StartupStatus>;
  catalogSwitch: CatalogSwitch;
}) {
  switch (startup.status) {
    case "loading":
      return null;
    case "error":
      return <StartupErrorView failure={{ kind: "coreUnreachable" }} />;
    case "ready":
      return startup.data.status === "ready" ? (
        <CatalogScreen workspaceDir={startup.data.workspaceDir} {...library} />
      ) : (
        <StartupErrorView
          failure={startup.data.failure}
          catalogActions={catalogSwitch}
        />
      );
  }
}

/** Bibliothèque si le catalogue contient des éléments, accueil sinon. */
function CatalogScreen({
  workspaceDir,
  onImport,
  importKey,
  thumbnailsKey,
  editsKey,
  trashKey,
  placesKey,
  triage,
}: LibraryInputs & { workspaceDir: string }) {
  // Les compteurs ne font qu'augmenter : leur somme change dès que l'un
  // d'eux change.
  const contentsKey = importKey + editsKey + trashKey + placesKey;
  const overview = useBackendData(
    fetchCatalogOverview,
    "catalog overview",
    contentsKey,
  );

  if (overview.status === "loading") return null;
  if (
    overview.status === "ready" &&
    overview.data.photoCount + overview.data.videoCount > 0
  ) {
    return (
      <LibraryView
        overview={overview.data}
        refreshKey={contentsKey + thumbnailsKey}
        onImport={onImport}
        triage={triage}
      />
    );
  }
  return (
    <WelcomeView
      workspaceDir={workspaceDir}
      overview={overview}
      onImport={onImport}
    />
  );
}
