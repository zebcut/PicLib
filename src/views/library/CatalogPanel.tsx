import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { formatCount } from "../../i18n/format";
import type { CatalogOverview } from "../../services/catalogInfo";
import type { FolderDto } from "../../services/folders";
import type { KeywordListItemDto } from "../../services/keywordList";
import type { GridSource } from "../../services/library";
import type { CatalogEdits } from "../../state/useCatalogEdits";
import type { CollectionsState } from "../../state/useCollections";
import type { FolderPanelState } from "../../state/useFolderPanel";
import { CollectionPanel } from "./CollectionPanel";
import { FolderPanel } from "./FolderPanel";
import type { AutoStackKind } from "./AutoStackDialog";
import type { FolderCommand } from "./FolderPanel";
import styles from "./LibraryView.module.css";

/**
 * Panneau de gauche : ensembles de photos à afficher (tout le catalogue, le
 * dernier import, et le mot-clé choisi dans la liste des mots-clés), le
 * panneau Dossiers et le bouton d'import.
 *
 * @param keyword mot-clé dont les photos sont affichées, s'il y en a un.
 * @param width largeur de la colonne, en pixels (réglée en glissant son
 *   bord).
 * @param folders contenu du panneau Dossiers, sous les ensembles.
 * @param onDeleteFolder ouvre « Supprimer le dossier… » ; absent, rien
 *   n'est proposé.
 * @param onMoveFolder ouvre la confirmation du déplacement d'un dossier
 *   glissé sur un autre ; absent, les dossiers ne se glissent pas.
 * @param onAutoStack ouvre « Empiler les paires RAW + JPEG… » ou
 *   « Empiler les rafales… » sur un dossier ; absent, rien n'est proposé.
 * @param collections contenu du panneau Collections, sous les dossiers.
 * @param selectedIds photos sélectionnées (proposées à la création d'une
 *   collection).
 * @param onChangeCollections absent : les collections ne se modifient pas.
 * @param onSetTarget choisit la collection cible (`null` : la collection
 *   rapide).
 */
export function CatalogPanel({
  width,
  folders,
  overview,
  activeSource,
  keyword,
  onSelectSource,
  onDeleteFolder,
  onMoveFolder,
  onAutoStack,
  folderCommands,
  onFolderCommand,
  onDropPhotos,
  collections,
  selectedIds,
  onChangeCollections,
  onSetTarget,
  keywords,
  onImport,
}: {
  width: number;
  folders: FolderPanelState;
  overview: CatalogOverview;
  activeSource: GridSource;
  keyword: KeywordListItemDto | null;
  onSelectSource: (source: GridSource) => void;
  onDeleteFolder?: (folderId: number) => void;
  onMoveFolder?: (folderId: number, destinationId: number) => void;
  onAutoStack?: (
    folderId: number,
    folderName: string,
    kind: AutoStackKind,
  ) => void;
  /** Photos déposées sur un dossier (voir `FolderPanel`). */
  onDropPhotos?: (mediaIds: number[], folderId: number) => void;
  /** Commandes du menu d'un dossier (voir `FolderPanel`). */
  folderCommands?: readonly FolderCommand[];
  onFolderCommand?: (
    command: FolderCommand,
    folder: { id: number; name: string },
  ) => void;
  collections: CollectionsState;
  selectedIds: readonly number[];
  onChangeCollections?: CatalogEdits["changeCollections"];
  onSetTarget?: (collectionId: number | null) => void;
  /** Mots-clés proposés dans les règles des collections dynamiques. */
  keywords: readonly KeywordListItemDto[];
  onImport: () => void;
}) {
  const quick = collections.status === "ready" ? collections.quick : null;
  const { t } = useTranslation();

  return (
    <nav
      className={styles.panel}
      style={{ width }}
      aria-label={t("library.panelTitle")}
    >
      <h2 className={styles.panelTitle}>{t("library.panelTitle")}</h2>
      <ul className={styles.sources}>
        <li>
          <SourceButton
            label={t("library.allPhotos")}
            count={overview.photoCount + overview.videoCount}
            isActive={activeSource.kind === "all"}
            onClick={() => {
              onSelectSource({ kind: "all" });
            }}
          />
        </li>
        <li>
          <SourceButton
            label={t("library.latestImport")}
            count={overview.latestImportItems}
            isActive={activeSource.kind === "latestImport"}
            onClick={() => {
              onSelectSource({ kind: "latestImport" });
            }}
          />
        </li>
        <li>
          <SourceButton
            label={
              quick?.targetId === null
                ? `${t("library.quickCollection")} +`
                : t("library.quickCollection")
            }
            count={quick?.mediaCount ?? 0}
            isActive={activeSource.kind === "quickCollection"}
            onClick={() => {
              onSelectSource({ kind: "quickCollection" });
            }}
          />
        </li>
        {(overview.missingCount > 0 || activeSource.kind === "missing") && (
          <li>
            <SourceButton
              label={t("library.missingPhotos")}
              count={overview.missingCount}
              isActive={activeSource.kind === "missing"}
              onClick={() => {
                onSelectSource({ kind: "missing" });
              }}
            />
          </li>
        )}
        {keyword !== null && (
          <li>
            <SourceButton
              label={t("library.keywordSource", { name: keyword.name })}
              count={keyword.photoCount}
              isActive={activeSource.kind === "keyword"}
              onClick={() => {
                onSelectSource({ kind: "keyword", keywordId: keyword.id });
              }}
            />
          </li>
        )}
      </ul>
      <FolderPanel
        state={folders}
        activeFolderId={
          activeSource.kind === "folder" ? activeSource.folderId : null
        }
        onSelectFolder={(folderId) => {
          onSelectSource({ kind: "folder", folderId });
        }}
        onDeleteFolder={onDeleteFolder}
        onAutoStack={onAutoStack}
        folderCommands={folderCommands}
        onFolderCommand={onFolderCommand}
        onDropPhotos={onDropPhotos}
        onMoveFolder={onMoveFolder}
      />
      <CollectionPanel
        state={collections}
        activeId={
          activeSource.kind === "collection" ? activeSource.collectionId : null
        }
        selectedIds={selectedIds}
        onSelect={(collectionId) => {
          onSelectSource({ kind: "collection", collectionId });
        }}
        onChange={onChangeCollections}
        onSetTarget={onSetTarget}
        keywords={keywords}
        folders={
          folders.status === "ready" ? folders.panel.folders : NO_FOLDERS
        }
      />
      <div className={styles.panelFooter}>
        <Button onClick={onImport} title={t("library.importTooltip")}>
          {t("library.importButton")}
        </Button>
      </div>
    </nav>
  );
}

/** Liste vide, stable d'un affichage à l'autre. */
const NO_FOLDERS: readonly FolderDto[] = [];

/** Ensemble du panneau ; indisponible (grisé) sans nombre d'éléments. */
function SourceButton({
  label,
  count,
  isActive,
  onClick,
}: {
  label: string;
  count: number | null;
  isActive: boolean;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={styles.source}
      aria-current={isActive ? "true" : undefined}
      disabled={count === null}
      onClick={onClick}
    >
      <span>{label}</span>
      {count !== null && (
        <span className={styles.sourceCount}>{formatCount(count)}</span>
      )}
    </button>
  );
}
