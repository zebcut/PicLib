import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  AUTOSCROLL_ATTRIBUTE,
  dropTargetAttributes,
  isHovered,
  useDragStart,
  useDragState,
  useDropHandler,
} from "../../components/DragProvider";
import { PanelSection } from "../../components/PanelSection";
import { TREE_TWISTY_ATTRIBUTE, Tree } from "../../components/Tree";
import { formatCount } from "../../i18n/format";
import { popUpContextMenu } from "../../menu/contextMenu";
import type { FolderDto } from "../../services/folders";
import type { FolderPanelState } from "../../state/useFolderPanel";
import { useStoredIdSet } from "../../state/useStoredIdSet";
import { buildTree, visibleRows } from "../../utils/tree";
import type { AutoStackKind } from "./AutoStackDialog";
import styles from "./FolderPanel.module.css";
import { canMoveInto } from "./folderMove";

/**
 * Commande du menu d'un dossier : relocaliser, synchroniser,
 * renommer, ou créer un sous-dossier pour les photos sélectionnées
 *.
 */
export type FolderCommand = "relocate" | "synchronize" | "rename" | "newFolder";

/** Clé sous laquelle l'état replié de la section est retenu. */
const SECTION_STORAGE_KEY = "piclib.panel.folders.open";

/**
 * Clé des branches dont l'état a été changé : un disque présent est
 * replié (ils sont dépliés par défaut), un dossier présent est déplié (ils
 * sont repliés par défaut).
 */
const TOGGLED_STORAGE_KEY = "piclib.panel.folders.toggled";

/** Ligne de l'arbre : un disque (identifiant négatif) ou un dossier. */
interface FolderItem {
  id: number;
  parentId: number | null;
  name: string;
  isDisk: boolean;
  /** Chemin complet, ou racine du disque. */
  path: string;
  mediaCount: number;
  /** Disque branché (pour un dossier : celui de son disque). */
  online: boolean;
}

/**
 * Identifiant d'un disque dans l'arbre : négatif, pour ne pas croiser ceux
 * des dossiers.
 */
function diskId(index: number): number {
  return -(index + 1);
}

/**
 * Panneau « Dossiers » : tous les dossiers du catalogue, même
 * vides, regroupés par disque (un disque débranché
 * est signalé), avec le nombre de photos de chacun, sous-dossiers compris. Un
 * clic sur un dossier (ou Espace) affiche ses photos et celles de ses
 * sous-dossiers. Son menu contextuel, Suppr ou ⌫ proposent de le supprimer
 * ; le glisser sur un autre dossier le déplace.
 */
export function FolderPanel({
  state,
  activeFolderId,
  onSelectFolder,
  onDeleteFolder,
  onMoveFolder,
  onAutoStack,
  folderCommands = [],
  onFolderCommand,
  onDropPhotos,
}: {
  state: FolderPanelState;
  /** Dossier affiché dans la grille, s'il y en a un. */
  activeFolderId: number | null;
  onSelectFolder: (folderId: number) => void;
  /** Ouvre « Supprimer le dossier… » ; absent, rien n'est proposé. */
  onDeleteFolder?: (folderId: number) => void;
  /**
   * Ouvre la confirmation du déplacement d'un dossier déposé sur
   * `destinationId` ; absent, les dossiers ne se glissent pas.
   */
  onMoveFolder?: (folderId: number, destinationId: number) => void;
  /**
   * Ouvre « Empiler les paires RAW + JPEG… » ou « Empiler les rafales… »
   * sur un dossier et ses sous-dossiers ; absent, rien n'est proposé.
   */
  onAutoStack?: (
    folderId: number,
    folderName: string,
    kind: AutoStackKind,
  ) => void;
  /**
   * Ouvre la confirmation du déplacement de photos déposées sur un dossier ;
   * absent, les photos ne s'y déposent pas.
   */
  onDropPhotos?: (mediaIds: number[], folderId: number) => void;
  /** Commandes proposées dans le menu d'un dossier, dans cet ordre. */
  folderCommands?: readonly FolderCommand[];
  /** Lance une commande du menu d'un dossier. */
  onFolderCommand?: (
    command: FolderCommand,
    folder: { id: number; name: string },
  ) => void;
}) {
  const { t, i18n } = useTranslation();
  const [toggled, setToggled] = useStoredIdSet(TOGGLED_STORAGE_KEY);
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(
    null,
  );
  const startDrag = useDragStart();
  const drag = useDragState();

  const items = useMemo((): FolderItem[] => {
    if (state.status !== "ready") return [];
    const { disks, folders } = state.panel;
    return [
      ...disks.map((disk, index) => ({
        id: diskId(index),
        parentId: null,
        name: disk.name ?? t("folders.startupDisk"),
        isDisk: true,
        path: disk.root,
        mediaCount: disk.mediaCount,
        online: disk.online,
      })),
      ...folders.map((folder) => ({
        id: folder.id,
        parentId: folder.parentId ?? diskId(folder.disk),
        name: folder.name,
        isDisk: false,
        path: folder.path,
        mediaCount: folder.mediaCount,
        online: disks[folder.disk]?.online ?? true,
      })),
    ];
  }, [state, t]);
  const folderById = useMemo(
    () =>
      new Map<number, FolderDto>(
        state.status === "ready"
          ? state.panel.folders.map((folder) => [folder.id, folder])
          : [],
      ),
    [state],
  );
  const tree = useMemo(() => buildTree(items), [items]);
  // Les disques restent dans leur ordre (démarrage d'abord) : seuls les
  // dossiers sont triés par nom.
  const roots = useMemo(
    () => [...tree].sort((a, b) => b.item.id - a.item.id),
    [tree],
  );
  const rows = useMemo(
    () =>
      visibleRows(roots, (id) => (id < 0 ? !toggled.has(id) : toggled.has(id))),
    [roots, toggled],
  );

  const toggle = (ids: number[], open: boolean) => {
    setToggled(
      ids.filter((id) => id < 0),
      !open,
    );
    setToggled(
      ids.filter((id) => id >= 0),
      open,
    );
  };

  useDropHandler(["folder"], {
    canDrop: (payload, target) => {
      if (target.kind !== "folder") return false;
      // Des photos glissées de la grille ou de la filmstrip.
      if (payload.kind === "photos") {
        return onDropPhotos !== undefined && folderById.has(target.folderId);
      }
      if (payload.kind !== "folder") return false;
      const source = folderById.get(payload.folderId);
      const destination = folderById.get(target.folderId);
      return (
        source !== undefined &&
        destination !== undefined &&
        onMoveFolder !== undefined &&
        canMoveInto(source, destination)
      );
    },
    drop: (payload, target) => {
      if (target.kind !== "folder") return;
      if (payload.kind === "folder") {
        onMoveFolder?.(payload.folderId, target.folderId);
      } else if (payload.kind === "photos") {
        onDropPhotos?.(payload.mediaIds, target.folderId);
      }
    },
  });

  const openMenu = (item: FolderItem) => {
    if (item.isDisk) return;
    const entries = [
      ...(onFolderCommand === undefined
        ? []
        : folderCommands.map((command) => ({
            id: `folder.${command}`,
            text: t(`folders.command.${command}`),
            action: () => {
              onFolderCommand(command, { id: item.id, name: item.name });
            },
          }))),
      ...(onAutoStack === undefined
        ? []
        : [
            {
              id: "folder.stackPairs",
              text: t("folders.menuStackPairs"),
              action: () => {
                onAutoStack(item.id, item.name, "pairs");
              },
            },
            {
              id: "folder.stackBursts",
              text: t("folders.menuStackBursts"),
              action: () => {
                onAutoStack(item.id, item.name, "bursts");
              },
            },
          ]),
      ...(onDeleteFolder === undefined
        ? []
        : [
            {
              id: "folder.delete",
              text: t("folders.menuDelete"),
              action: () => {
                onDeleteFolder(item.id);
              },
            },
          ]),
    ];
    if (entries.length === 0) return;
    popUpContextMenu(entries).catch((error: unknown) => {
      console.error("Failed to open the folder menu", error);
    });
  };

  const content = () => {
    switch (state.status) {
      case "loading":
        return null;
      case "failed":
        return (
          <p className={styles.message} role="alert">
            {t("folders.loadFailed")}
          </p>
        );
      case "ready":
        break;
    }
    if (rows.length === 0) {
      return <p className={styles.message}>{t("folders.empty")}</p>;
    }
    return (
      <div
        ref={setScrollElement}
        className={styles.tree}
        {...AUTOSCROLL_ATTRIBUTE}
      >
        <Tree
          label={t("folders.title")}
          rows={rows}
          scrollElement={scrollElement}
          onToggle={toggle}
          onActivate={(item) => {
            if (!item.isDisk) onSelectFolder(item.id);
          }}
          onDelete={(item) => {
            if (!item.isDisk) onDeleteFolder?.(item.id);
          }}
          onRowContextMenu={openMenu}
          rowProps={({ node: { item } }) => ({
            ...(!item.isDisk &&
              dropTargetAttributes({ kind: "folder", folderId: item.id })),
            className: [
              item.isDisk ? styles.disk : undefined,
              item.id === activeFolderId ? styles.active : undefined,
              item.online ? undefined : styles.offline,
              isHovered(drag, { kind: "folder", folderId: item.id })
                ? styles.dropTarget
                : undefined,
            ]
              .filter((name) => name !== undefined)
              .join(" "),
            "aria-selected": item.isDisk
              ? undefined
              : item.id === activeFolderId,
            onPointerDown: (event) => {
              if (item.isDisk || onMoveFolder === undefined) return;
              const origin = event.target as HTMLElement;
              if (origin.closest(`[${TREE_TWISTY_ATTRIBUTE}]`) !== null) return;
              startDrag(event, () => ({
                kind: "folder",
                folderId: item.id,
                label: item.name,
              }));
            },
            onClick: (event) => {
              // Le triangle déplie sans afficher le dossier.
              const origin = event.target as HTMLElement;
              if (origin.closest(`[${TREE_TWISTY_ATTRIBUTE}]`) !== null) return;
              if (!item.isDisk) onSelectFolder(item.id);
            },
          })}
          renderRow={({ node: { item } }) => (
            <>
              <span
                className={styles.name}
                title={
                  item.online
                    ? item.path
                    : `${item.path}\n${t("folders.offlineHint")}`
                }
              >
                {item.name}
              </span>
              {item.isDisk && !item.online && (
                <span className={styles.badge}>{t("folders.offline")}</span>
              )}
              <span className={styles.count}>
                {formatCount(item.mediaCount, i18n.language)}
              </span>
            </>
          )}
        />
      </div>
    );
  };

  return (
    <PanelSection
      title={t("folders.title")}
      storageKey={SECTION_STORAGE_KEY}
      className={styles.section}
      bodyClassName={styles.body}
    >
      {content()}
    </PanelSection>
  );
}
