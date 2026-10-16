import { useEffect, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { DragProvider } from "../../components/DragProvider";
import { ResizeHandle } from "../../components/ResizeHandle";
import type { CatalogOverview } from "../../services/catalogInfo";
import type { KeywordListItemDto } from "../../services/keywordList";
import type { GridQuery, GridSource } from "../../services/library";
import type { MapArea } from "../../services/map";
import { counted } from "../../i18n/format";
import { checkMissingFiles } from "../../services/library";
import { checkCatalog } from "../../services/maintenance";
import { chooseRelocatedFolder } from "../../services/relocation";
import { checkFolderSidecars } from "../../services/xmp";
import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import {
  activeFilter,
  NO_FILTER_BAR,
  SORT_KEYS,
  toggleEnabled,
} from "../../state/gridFilter";
import type { FilterBarState } from "../../state/gridFilter";
import { setCollectionTarget } from "../../services/collections";
import { useCollections } from "../../state/useCollections";
import { useFolderPanel } from "../../state/useFolderPanel";
import { useKeywordList } from "../../state/useKeywordList";
import { useKeywordSets } from "../../state/useKeywordSets";
import { useMetadataTemplates } from "../../state/useMetadataTemplates";
import { useStoredBoolean } from "../../state/useStoredBoolean";
import { useStoredNumber } from "../../state/useStoredNumber";
import { useSourceFilterMemory } from "../../state/useSourceFilterMemory";
import { AutoStackDialog } from "./AutoStackDialog";
import type { FolderCommand } from "./FolderPanel";
import { RelocateFolderDialog } from "./RelocateFolderDialog";
import { SyncFolderDialog } from "./SyncFolderDialog";
import { FolderNameDialog } from "./FolderNameDialog";
import { MoveMediaDialog } from "./MoveMediaDialog";
import { OperationsLogDialog } from "./OperationsLogDialog";
import type { AutoStackRequest } from "./AutoStackDialog";
import { CatalogPanel } from "./CatalogPanel";
import { QuickCollectionSaveDialog } from "./CollectionDialogs";
import { COLUMN_WIDTH } from "./columns";
import { FolderDeleteDialog } from "./FolderDeleteDialog";
import { FolderMoveDialog } from "./FolderMoveDialog";
import { CELL_SIZE } from "./GridToolbar";
import { LibraryBrowser } from "./LibraryBrowser";
import type { EditActions } from "./LibraryBrowser";
import styles from "./LibraryView.module.css";

/** Liste vide, stable d'un affichage à l'autre. */
const NO_KEYWORDS: readonly KeywordListItemDto[] = [];

/** Clé de la taille des vignettes retenue d'une session à l'autre. */
const CELL_SIZE_STORAGE_KEY = "piclib.grid.cellSize";

/** Clés du tri et de la barre de filtres, retenus d'une session à l'autre. */
const SORT_KEY_STORAGE_KEY = "piclib.grid.sortKey";
const SORT_DESCENDING_STORAGE_KEY = "piclib.grid.sortDescending";
const FILTER_BAR_STORAGE_KEY = "piclib.grid.filterBarVisible";
const FILTER_LOCK_STORAGE_KEY = "piclib.grid.filterLocked";

/**
 * Tout le catalogue. Un objet unique : la grille relit ses éléments quand
 * sa source change d'identité.
 */
const ALL: GridSource = { kind: "all" };

/**
 * Bibliothèque : le panneau Catalogue à gauche ; à droite, la Grille ou la
 * Loupe, avec sa barre d'outils. La source affichée est tout le catalogue,
 * le dernier import, les photos d'un mot-clé (flèche de la liste des
 * mots-clés) ou d'un dossier ; filtrée par la barre de filtres, triée selon
 * le tri retenu d'une session à l'autre.
 *
 * @param refreshKey les éléments sont relus quand cette valeur change
 *   (import terminé, nouvelles vignettes, modification du catalogue).
 */
export function LibraryView({
  overview,
  refreshKey,
  onImport,
  triage,
}: {
  overview: CatalogOverview;
  refreshKey: number;
  onImport: () => void;
  triage: EditActions;
}) {
  const { t } = useTranslation();
  const [chosenSource, setChosenSource] = useState<GridSource>(ALL);
  // Journal des opérations ouvert.
  const [isLogOpen, setIsLogOpen] = useState(false);
  // Barre de filtres : repart de « Aucun » à chaque nouvelle source, sauf
  // cadenas fermé ou filtre mémorisé pour la source.
  const [filterBar, setFilterBar] = useState<FilterBarState>(NO_FILTER_BAR);
  const [isFilterLocked, setFilterLocked] = useStoredBoolean(
    FILTER_LOCK_STORAGE_KEY,
    false,
  );
  const [isFilterBarVisible, setFilterBarVisible] = useStoredBoolean(
    FILTER_BAR_STORAGE_KEY,
    true,
  );
  const [sortIndex, setSortIndex] = useStoredNumber(
    SORT_KEY_STORAGE_KEY,
    0,
    0,
    SORT_KEYS.length - 1,
  );
  const [isSortDescending, setSortDescending] = useStoredBoolean(
    SORT_DESCENDING_STORAGE_KEY,
    true,
  );
  const changeFilterSource = useSourceFilterMemory({
    source: chosenSource,
    filterBar,
    setFilterBar,
    isLocked: isFilterLocked,
  });
  const showSource = (next: GridSource) => {
    setChosenSource(next);
    changeFilterSource(next);
  };
  useShortcuts({
    // La barre se masque sans que les filtres cessent d'agir.
    "filters.toggleBar": () => {
      setFilterBarVisible(!isFilterBarVisible);
    },
    "filters.toggleEnabled": () => {
      setFilterBar(toggleEnabled);
    },
    "collection.showQuick": () => {
      showSource({ kind: "quickCollection" });
    },
    "library.operationsLog": () => {
      setIsLogOpen(true);
    },
    "library.findMissing": () => {
      checkMissingFiles().catch((error: unknown) => {
        console.error("Failed to look for the missing files", error);
      });
    },
    "catalog.cleanUp": () => {
      checkCatalog().catch((error: unknown) => {
        console.error("Failed to check the catalog", error);
      });
    },
    "collection.clearQuick": () => {
      void triage.changeCollections?.({ kind: "clearQuick" });
    },
    "collection.saveQuick": () => {
      if (triage.changeCollections !== undefined) setIsSavingQuick(true);
    },
  });
  // Dossier dont la suppression est demandée (panneau Dossiers).
  const [deletingFolderId, setDeletingFolderId] = useState<number | null>(null);
  // Change après chaque dossier supprimé : la Grille repart sans sélection,
  // ses photos ont pu quitter le catalogue.
  const [folderDeletions, setFolderDeletions] = useState(0);
  // Dossier glissé sur un autre : déplacement à confirmer.
  const [moving, setMoving] = useState<{
    folderId: number;
    destinationId: number;
  } | null>(null);
  const { removeFolder, trashFolder, moveFolder } = triage;
  const column = COLUMN_WIDTH.left;
  const [columnWidth, setColumnWidth] = useStoredNumber(
    column.storageKey,
    column.default,
    column.min,
    column.max,
  );
  const [cellSize, setCellSize] = useStoredNumber(
    CELL_SIZE_STORAGE_KEY,
    CELL_SIZE.default,
    CELL_SIZE.min,
    CELL_SIZE.max,
  );
  const keywordList = useKeywordList(refreshKey);
  const keywordSets = useKeywordSets(refreshKey, keywordList.reload);
  const metadataTemplates = useMetadataTemplates(
    refreshKey,
    keywordList.reload,
  );
  const keywords =
    keywordList.state.status === "ready" ? keywordList.state.keywords : null;
  const shownKeyword =
    chosenSource.kind === "keyword"
      ? keywords?.find((keyword) => keyword.id === chosenSource.keywordId)
      : undefined;
  const folderPanel = useFolderPanel(refreshKey);
  // Change à chaque collection cible choisie : un réglage, hors de
  // l'historique, qui change pourtant le rond des vignettes.
  const [targetVersion, setTargetVersion] = useState(0);
  const collectionPanel = useCollections(refreshKey, targetVersion);
  // XMP modifiés par un autre logiciel, cherchés à l'affichage d'un dossier
  // : la grille est relue s'il y en a.
  const [sidecarVersion, setSidecarVersion] = useState(0);
  const shownFolderId =
    chosenSource.kind === "folder" ? chosenSource.folderId : null;
  useEffect(() => {
    if (shownFolderId === null) return;
    let isActive = true;
    checkFolderSidecars(shownFolderId).then(
      (report) => {
        if (isActive && report.changed + report.removed > 0) {
          setSidecarVersion((version) => version + 1);
        }
      },
      (error: unknown) => {
        console.error("Failed to check the folder's XMP files", error);
      },
    );
    return () => {
      isActive = false;
    };
  }, [shownFolderId]);
  const [isSavingQuick, setIsSavingQuick] = useState(false);
  // Empilement automatique à confirmer.
  const [autoStack, setAutoStack] = useState<AutoStackRequest | null>(null);
  // Dossier à relocaliser, une fois son nouvel emplacement choisi.
  const [relocating, setRelocating] = useState<{
    folderId: number;
    name: string;
    path: string;
  } | null>(null);
  // Dossier à synchroniser.
  const [synchronizing, setSynchronizing] = useState<{
    id: number;
    name: string;
  } | null>(null);
  // Photos sélectionnées dans la bibliothèque, pour le panneau Collections.
  const [selectedIds, setSelectedIds] = useState<readonly number[]>([]);
  // Photos déposées sur un dossier, à confirmer.
  const [movingMedia, setMovingMedia] = useState<{
    mediaIds: number[];
    folderId: number;
  } | null>(null);
  // Dossier à créer (avec les photos sélectionnées) ou à renommer.
  const [naming, setNaming] = useState<{
    kind: "newFolder" | "rename";
    id: number;
    name: string;
  } | null>(null);
  const folderCommands: FolderCommand[] = [
    ...(triage.relocateFolder === undefined ? [] : ["relocate" as const]),
    ...(triage.removePhotos === undefined ? [] : ["synchronize" as const]),
    ...(triage.renameFolder === undefined ? [] : ["rename" as const]),
    ...(triage.moveMediaToNewFolder === undefined || selectedIds.length === 0
      ? []
      : ["newFolder" as const]),
  ];
  const runFolderCommand = (
    command: FolderCommand,
    folder: { id: number; name: string },
  ) => {
    switch (command) {
      case "relocate":
        chooseRelocatedFolder(
          t("relocation.folderDialogTitle", { name: folder.name }),
        ).then(
          (path) => {
            if (path !== null) {
              setRelocating({ folderId: folder.id, name: folder.name, path });
            }
          },
          (error: unknown) => {
            console.error("Failed to choose the folder", error);
          },
        );
        break;
      case "synchronize":
        setSynchronizing(folder);
        break;
      case "rename":
      case "newFolder":
        setNaming({ kind: command, ...folder });
        break;
    }
  };
  const setTarget = (collectionId: number | null) => {
    setCollectionTarget(collectionId).then(
      () => {
        setTargetVersion((previous) => previous + 1);
      },
      (error: unknown) => {
        console.error("Failed to set the collection target", error);
      },
    );
  };
  const collections =
    collectionPanel.status === "ready" ? collectionPanel.collections : null;
  const shownCollection =
    chosenSource.kind === "collection"
      ? collections?.find(
          (collection) => collection.id === chosenSource.collectionId,
        )
      : undefined;
  const folders =
    folderPanel.status === "ready" ? folderPanel.panel.folders : null;
  const shownFolder =
    chosenSource.kind === "folder"
      ? folders?.find((folder) => folder.id === chosenSource.folderId)
      : undefined;

  // Sans import (catalogue ouvert d'une ancienne version), ou si le
  // mot-clé ou le dossier affiché a disparu, tout est affiché.
  const source: GridSource =
    (chosenSource.kind === "latestImport" &&
      overview.latestImportItems === null) ||
    (chosenSource.kind === "keyword" &&
      keywords !== null &&
      shownKeyword === undefined) ||
    (chosenSource.kind === "folder" &&
      folders !== null &&
      shownFolder === undefined) ||
    (chosenSource.kind === "collection" &&
      collections !== null &&
      shownCollection === undefined)
      ? ALL
      : chosenSource;
  const itemCount = (() => {
    switch (source.kind) {
      case "all":
        return overview.photoCount + overview.videoCount;
      case "latestImport":
        return overview.latestImportItems ?? 0;
      case "keyword":
        return shownKeyword?.photoCount ?? 0;
      case "folder":
        return shownFolder?.mediaCount ?? 0;
      case "collection":
        return shownCollection?.mediaCount ?? 0;
      case "quickCollection":
        return collectionPanel.status === "ready"
          ? collectionPanel.quick.mediaCount
          : 0;
      case "missing":
        return overview.missingCount;
    }
  })();
  // « Limiter à la zone visible » de la carte : à part des
  // filtres de la barre, il n'est enregistré ni dans un préréglage ni pour
  // la source.
  const [mapArea, setMapArea] = useState<MapArea | null>(null);
  const barFilter = activeFilter(filterBar);
  const filter = useMemo(
    () => (mapArea === null ? barFilter : { ...barFilter, area: mapArea }),
    [barFilter, mapArea],
  );
  const sortKey = SORT_KEYS[sortIndex] ?? "captureTime";
  // Un nouvel objet seulement si la source, le filtre ou le tri change : la
  // grille relit alors tout.
  const query = useMemo<GridQuery>(
    () => ({
      source,
      filter,
      sort: { key: sortKey, descending: isSortDescending },
    }),
    [source, filter, sortKey, isSortDescending],
  );
  const sourceKey = `${(() => {
    switch (source.kind) {
      case "keyword":
        return `keyword-${String(source.keywordId)}`;
      case "folder":
        return `folder-${String(source.folderId)}`;
      case "collection":
        return `collection-${String(source.collectionId)}`;
      default:
        return source.kind;
    }
  })()}-${String(folderDeletions)}`;

  return (
    <DragProvider>
      <div className={styles.layout}>
        <CatalogPanel
          width={columnWidth}
          folders={folderPanel}
          overview={overview}
          activeSource={source}
          keyword={shownKeyword ?? null}
          onSelectSource={showSource}
          onDeleteFolder={
            removeFolder === undefined || trashFolder === undefined
              ? undefined
              : setDeletingFolderId
          }
          onMoveFolder={
            moveFolder === undefined
              ? undefined
              : (folderId, destinationId) => {
                  setMoving({ folderId, destinationId });
                }
          }
          collections={collectionPanel}
          selectedIds={selectedIds}
          onAutoStack={
            triage.changeStacks === undefined
              ? undefined
              : (folderId, folderName, kind) => {
                  setAutoStack({
                    scope: { kind: "folder", folderId },
                    folderName,
                    kind,
                  });
                }
          }
          folderCommands={folderCommands}
          onFolderCommand={runFolderCommand}
          onDropPhotos={
            triage.moveMedia === undefined
              ? undefined
              : (mediaIds, folderId) => {
                  setMovingMedia({ mediaIds, folderId });
                }
          }
          onChangeCollections={triage.changeCollections}
          onSetTarget={setTarget}
          keywords={keywords ?? NO_KEYWORDS}
          onImport={onImport}
        />
        <ResizeHandle
          label={t("library.resizeLeftColumn")}
          orientation="vertical"
          direction={1}
          value={columnWidth}
          min={column.min}
          max={column.max}
          defaultValue={column.default}
          onChange={setColumnWidth}
        />
        <main className={styles.main}>
          {/* Une source neuve repart de la Grille, sans sélection. */}
          <LibraryBrowser
            key={sourceKey}
            query={query}
            mapArea={mapArea}
            onMapAreaChange={setMapArea}
            filterBar={filterBar}
            isFilterLocked={isFilterLocked}
            onFilterLockChange={setFilterLocked}
            isFilterBarVisible={isFilterBarVisible}
            onFilterBarChange={setFilterBar}
            onSelectionChange={setSelectedIds}
            onRemoveFromCollection={
              shownCollection?.kind === "collection" &&
              triage.changeCollections !== undefined
                ? (mediaIds) => {
                    void triage.changeCollections?.({
                      kind: "removeMedia",
                      collectionId: shownCollection.id,
                      mediaIds,
                    });
                  }
                : undefined
            }
            onSortChange={(sort) => {
              setSortIndex(Math.max(0, SORT_KEYS.indexOf(sort.key)));
              setSortDescending(sort.descending);
            }}
            keywordList={keywordList}
            keywordSets={keywordSets}
            metadataTemplates={metadataTemplates}
            onShowKeyword={(keywordId) => {
              showSource({ kind: "keyword", keywordId });
            }}
            // Les trois ne font que croître : leur somme change dès que l'un
            // change (le rond des vignettes suit la cible, l'icône XMP les
            // fichiers modifiés ailleurs).
            refreshKey={refreshKey + targetVersion + sidecarVersion}
            itemCount={itemCount}
            cellSize={cellSize}
            onCellSizeChange={setCellSize}
            triage={triage}
            onAutoStack={setAutoStack}
          />
        </main>
        {isLogOpen && (
          <OperationsLogDialog
            onClose={() => {
              setIsLogOpen(false);
            }}
          />
        )}
        {movingMedia !== null && triage.moveMedia !== undefined && (
          <MoveMediaDialog
            mediaIds={movingMedia.mediaIds}
            folderId={movingMedia.folderId}
            onMove={triage.moveMedia}
            onClose={() => {
              setMovingMedia(null);
            }}
          />
        )}
        {naming !== null && (
          <FolderNameDialog
            title={
              naming.kind === "rename"
                ? t("fileMove.renameTitle", { name: naming.name })
                : t("fileMove.newFolderTitle", { name: naming.name })
            }
            initialName={naming.kind === "rename" ? naming.name : ""}
            explanation={
              naming.kind === "newFolder"
                ? t(
                    "fileMove.newFolderExplanation",
                    counted(selectedIds.length),
                  )
                : undefined
            }
            submitLabel={
              naming.kind === "rename"
                ? t("fileMove.rename")
                : t("fileMove.create")
            }
            onSubmit={(name) => {
              if (naming.kind === "rename") {
                triage.renameFolder?.(naming.id, name);
              } else {
                triage.moveMediaToNewFolder?.(
                  [...selectedIds],
                  naming.id,
                  name,
                );
              }
            }}
            onClose={() => {
              setNaming(null);
            }}
          />
        )}
        {synchronizing !== null && triage.removePhotos !== undefined && (
          <SyncFolderDialog
            folderId={synchronizing.id}
            folderName={synchronizing.name}
            onRemove={triage.removePhotos}
            onReadMetadata={triage.readFileMetadata}
            onDone={() => {
              setSynchronizing(null);
              setSidecarVersion((version) => version + 1);
              // Les fichiers qui ont changé de place sont de nouveau
              // cherchés.
              checkMissingFiles().catch((error: unknown) => {
                console.error("Failed to look for the missing files", error);
              });
            }}
            onClose={() => {
              setSynchronizing(null);
              // La synchronisation a aussi cherché les XMP modifiés ailleurs.
              setSidecarVersion((version) => version + 1);
            }}
          />
        )}
        {relocating !== null && triage.relocateFolder !== undefined && (
          <RelocateFolderDialog
            folderId={relocating.folderId}
            folderName={relocating.name}
            path={relocating.path}
            onRelocate={triage.relocateFolder}
            onClose={() => {
              setRelocating(null);
            }}
          />
        )}
        {autoStack !== null && triage.changeStacks !== undefined && (
          <AutoStackDialog
            request={autoStack}
            onStack={triage.changeStacks}
            onClose={() => {
              setAutoStack(null);
            }}
          />
        )}
        {deletingFolderId !== null &&
          removeFolder !== undefined &&
          trashFolder !== undefined && (
            <FolderDeleteDialog
              folderId={deletingFolderId}
              onRemove={removeFolder}
              onTrash={trashFolder}
              onDone={() => {
                setDeletingFolderId(null);
                setFolderDeletions((previous) => previous + 1);
              }}
              onClose={() => {
                setDeletingFolderId(null);
              }}
            />
          )}
        {isSavingQuick && triage.changeCollections !== undefined && (
          <QuickCollectionSaveDialog
            onSave={async (name, clear) => {
              const result = await triage.changeCollections?.({
                kind: "saveQuick",
                name,
                clear,
              });
              if (result?.refusal === null) setIsSavingQuick(false);
              return result?.refusal ?? null;
            }}
            onClose={() => {
              setIsSavingQuick(false);
            }}
          />
        )}
        {moving !== null && moveFolder !== undefined && (
          <FolderMoveDialog
            folderId={moving.folderId}
            destinationId={moving.destinationId}
            onMove={moveFolder}
            onDone={() => {
              setMoving(null);
            }}
            onClose={() => {
              setMoving(null);
            }}
          />
        )}
      </div>
    </DragProvider>
  );
}
