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
import type { DragPayload, DropTarget } from "../../components/DragProvider";
import { PanelSection } from "../../components/PanelSection";
import { TREE_TWISTY_ATTRIBUTE, Tree } from "../../components/Tree";
import { formatCount } from "../../i18n/format";
import { popUpContextMenu } from "../../menu/contextMenu";
import type {
  CollectionDto,
  CollectionKindDto,
} from "../../services/collections";
import type { CatalogEdits } from "../../state/useCatalogEdits";
import type { CollectionsState } from "../../state/useCollections";
import { useStoredIdSet } from "../../state/useStoredIdSet";
import type { FolderDto } from "../../services/folders";
import type { KeywordListItemDto } from "../../services/keywordList";
import { newSmartRules } from "../../state/smartRules";
import { buildTree, visibleRows } from "../../utils/tree";
import {
  CollectionNameDialog,
  DeleteCollectionFolderDialog,
} from "./CollectionDialogs";
import styles from "./FolderPanel.module.css";
import { SmartCollectionDialog } from "./SmartCollectionDialog";

/** Clé sous laquelle l'état replié de la section est retenu. */
const SECTION_STORAGE_KEY = "piclib.panel.collections.open";

/** Clé des dossiers de collections repliés (ils sont dépliés par défaut). */
const COLLAPSED_STORAGE_KEY = "piclib.panel.collections.collapsed";

/** Fenêtre ouverte : nom à donner, ou suppression d'un dossier à confirmer. */
type Request =
  | {
      kind: "create";
      collectionKind: CollectionKindDto;
      parent: CollectionDto | null;
    }
  | { kind: "createSmart"; parent: CollectionDto | null }
  | { kind: "editRules"; collection: CollectionDto }
  | { kind: "rename"; collection: CollectionDto }
  | { kind: "deleteFolder"; collection: CollectionDto };

/**
 * Panneau « Collections » : l'arbre des dossiers de
 * collections et des collections, avec le nombre de photos de chacun. Un
 * clic affiche les photos d'une collection, ou de toutes celles d'un
 * dossier. Le « + » et le menu contextuel créent, renomment, suppriment ;
 * glisser une collection sur un dossier (ou à côté de l'arbre, pour la
 * racine) la déplace ; glisser des photos sur une collection les y met.
 * Tout s'annule par Cmd+Z.
 *
 * @param selectedIds photos sélectionnées, proposées à la création d'une
 *   collection.
 * @param onChange absent : le panneau ne fait qu'afficher.
 * @param onSetTarget choisit la collection cible (`null` : la collection
 *   rapide), marquée d'un « + ».
 */
export function CollectionPanel({
  state,
  activeId,
  selectedIds,
  onSelect,
  onChange,
  onSetTarget,
  keywords,
  folders,
}: {
  state: CollectionsState;
  /** Collection ou dossier affiché dans la grille, s'il y en a un. */
  activeId: number | null;
  selectedIds: readonly number[];
  onSelect: (collectionId: number) => void;
  onChange?: CatalogEdits["changeCollections"];
  onSetTarget?: (collectionId: number | null) => void;
  /** Mots-clés proposés dans les règles des collections dynamiques. */
  keywords: readonly KeywordListItemDto[];
  /** Dossiers proposés dans les règles des collections dynamiques. */
  folders: readonly FolderDto[];
}) {
  const { t, i18n } = useTranslation();
  const [collapsed, setCollapsed] = useStoredIdSet(COLLAPSED_STORAGE_KEY);
  const [request, setRequest] = useState<Request | null>(null);
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(
    null,
  );
  const startDrag = useDragStart();
  const drag = useDragState();

  const collections = useMemo(
    () => (state.status === "ready" ? state.collections : []),
    [state],
  );
  const targetId = state.status === "ready" ? state.quick.targetId : null;
  const byId = useMemo(
    () => new Map(collections.map((collection) => [collection.id, collection])),
    [collections],
  );
  const roots = useMemo(() => buildTree(collections), [collections]);
  const rows = useMemo(
    () => visibleRows(roots, (id) => !collapsed.has(id)),
    [roots, collapsed],
  );

  /** Vrai si `id` est `ancestor` ou se trouve dans sa branche. */
  const isInside = (id: number, ancestor: number): boolean => {
    let current = byId.get(id);
    for (
      let depth = 0;
      current !== undefined && depth <= byId.size;
      depth += 1
    ) {
      if (current.id === ancestor) return true;
      current =
        current.parentId === null ? undefined : byId.get(current.parentId);
    }
    return false;
  };

  const canDrop = (payload: DragPayload, target: DropTarget): boolean => {
    if (onChange === undefined) return false;
    if (payload.kind === "photos") {
      // Les photos d'une collection dynamique sont calculées : on n'y
      // dépose rien.
      const destination =
        target.kind === "collection"
          ? byId.get(target.collectionId)
          : undefined;
      return destination?.kind === "collection" && destination.rules === null;
    }
    if (payload.kind !== "collection") return false;
    const moved = byId.get(payload.collectionId);
    if (moved === undefined) return false;
    if (target.kind === "collectionRoot") return moved.parentId !== null;
    if (target.kind !== "collection") return false;
    const destination = byId.get(target.collectionId);
    if (destination === undefined) return false;
    // A smart collection also goes under another smart one, which it then
    // refines.
    const accepts =
      destination.kind === "folder" ||
      (destination.rules !== null && moved.rules !== null);
    return (
      accepts &&
      destination.id !== moved.parentId &&
      !isInside(destination.id, moved.id)
    );
  };
  useDropHandler(["collection", "collectionRoot"], {
    canDrop,
    drop: (payload, target) => {
      if (onChange === undefined) return;
      if (payload.kind === "photos" && target.kind === "collection") {
        void onChange({
          kind: "addMedia",
          collectionId: target.collectionId,
          mediaIds: payload.mediaIds,
        });
      } else if (payload.kind === "collection") {
        void onChange({
          kind: "move",
          collectionId: payload.collectionId,
          parentId: target.kind === "collection" ? target.collectionId : null,
        });
      }
    },
  });

  /** Name of the smart parent `collection` refines, if any. */
  const smartParentName = (collection: CollectionDto): string | null => {
    const parent =
      collection.parentId === null ? undefined : byId.get(collection.parentId);
    return (parent?.rules ?? null) === null ? null : (parent?.name ?? null);
  };

  const remove = (collection: CollectionDto) => {
    if (onChange === undefined) return;
    const hasChildren = collections.some(
      (other) => other.parentId === collection.id,
    );
    if (hasChildren) setRequest({ kind: "deleteFolder", collection });
    else void onChange({ kind: "delete", collectionId: collection.id });
  };

  const openMenu = (collection: CollectionDto) => {
    if (onChange === undefined) return;
    const isFolder = collection.kind === "folder";
    popUpContextMenu([
      ...(isFolder
        ? [
            {
              text: t("collections.newCollectionIn", { name: collection.name }),
              action: () => {
                setRequest({
                  kind: "create",
                  collectionKind: "collection",
                  parent: collection,
                });
              },
            },
            {
              text: t("collections.newSmartIn", { name: collection.name }),
              action: () => {
                setRequest({ kind: "createSmart", parent: collection });
              },
            },
            {
              text: t("collections.newFolderIn", { name: collection.name }),
              action: () => {
                setRequest({
                  kind: "create",
                  collectionKind: "folder",
                  parent: collection,
                });
              },
            },
            { item: "Separator" as const },
          ]
        : []),
      ...(collection.rules === null
        ? []
        : [
            {
              text: t("collections.newSmartIn", { name: collection.name }),
              action: () => {
                setRequest({ kind: "createSmart", parent: collection });
              },
            },
            {
              text: t("collections.editRules"),
              action: () => {
                setRequest({ kind: "editRules", collection });
              },
            },
            { item: "Separator" as const },
          ]),
      ...(isFolder || collection.rules !== null || onSetTarget === undefined
        ? []
        : [
            {
              text:
                collection.id === targetId
                  ? t("collections.unsetTarget")
                  : t("collections.setTarget"),
              action: () => {
                onSetTarget(collection.id === targetId ? null : collection.id);
              },
            },
            { item: "Separator" as const },
          ]),
      {
        text: t("collections.rename"),
        action: () => {
          setRequest({ kind: "rename", collection });
        },
      },
      {
        text: isFolder
          ? t("collections.deleteFolder")
          : t("collections.delete"),
        action: () => {
          remove(collection);
        },
      },
    ]).catch((error: unknown) => {
      console.error("Failed to open the collection menu", error);
    });
  };

  const openAddMenu = () => {
    popUpContextMenu([
      {
        text: t("collections.newCollection"),
        action: () => {
          setRequest({
            kind: "create",
            collectionKind: "collection",
            parent: null,
          });
        },
      },
      {
        text: t("collections.newSmart"),
        action: () => {
          setRequest({ kind: "createSmart", parent: null });
        },
      },
      {
        text: t("collections.newFolder"),
        action: () => {
          setRequest({
            kind: "create",
            collectionKind: "folder",
            parent: null,
          });
        },
      },
    ]).catch((error: unknown) => {
      console.error("Failed to open the collection menu", error);
    });
  };

  const content = () => {
    switch (state.status) {
      case "loading":
        return null;
      case "failed":
        return (
          <p className={styles.message} role="alert">
            {t("collections.loadFailed")}
          </p>
        );
      case "ready":
        break;
    }
    return (
      <div
        ref={setScrollElement}
        className={[
          styles.tree,
          isHovered(drag, { kind: "collectionRoot" })
            ? styles.dropTarget
            : undefined,
        ]
          .filter((name) => name !== undefined)
          .join(" ")}
        {...AUTOSCROLL_ATTRIBUTE}
        {...dropTargetAttributes({ kind: "collectionRoot" })}
      >
        {rows.length === 0 ? (
          <p className={styles.message}>{t("collections.empty")}</p>
        ) : (
          <Tree
            label={t("collections.title")}
            rows={rows}
            scrollElement={scrollElement}
            onToggle={(ids, open) => {
              setCollapsed(ids, !open);
            }}
            onActivate={(item) => {
              onSelect(item.id);
            }}
            onDelete={remove}
            onRowContextMenu={openMenu}
            rowProps={({ node: { item } }) => ({
              ...dropTargetAttributes({
                kind: "collection",
                collectionId: item.id,
              }),
              className: [
                item.id === activeId ? styles.active : undefined,
                isHovered(drag, { kind: "collection", collectionId: item.id })
                  ? styles.dropTarget
                  : undefined,
              ]
                .filter((name) => name !== undefined)
                .join(" "),
              "aria-selected": item.id === activeId,
              onPointerDown: (event) => {
                if (onChange === undefined) return;
                const origin = event.target as HTMLElement;
                if (origin.closest(`[${TREE_TWISTY_ATTRIBUTE}]`) !== null)
                  return;
                startDrag(event, () => ({
                  kind: "collection",
                  collectionId: item.id,
                  label: item.name,
                }));
              },
              onClick: (event) => {
                // Le triangle déplie sans afficher les photos.
                const origin = event.target as HTMLElement;
                if (origin.closest(`[${TREE_TWISTY_ATTRIBUTE}]`) !== null)
                  return;
                onSelect(item.id);
              },
            })}
            renderRow={({ node: { item } }) => (
              <>
                <span className={styles.name}>
                  {item.kind === "folder"
                    ? "▤ "
                    : item.rules === null
                      ? ""
                      : "⚙ "}
                  {item.name}
                  {item.id === targetId && (
                    <span
                      className={styles.badge}
                      title={t("collections.target")}
                      aria-label={t("collections.target")}
                    >
                      {" "}
                      +
                    </span>
                  )}
                </span>
                <span className={styles.count}>
                  {formatCount(item.mediaCount, i18n.language)}
                </span>
              </>
            )}
          />
        )}
      </div>
    );
  };

  return (
    <PanelSection
      title={t("collections.title")}
      storageKey={SECTION_STORAGE_KEY}
      className={styles.section}
      bodyClassName={styles.body}
      actions={
        onChange === undefined ? undefined : (
          <button
            type="button"
            className={styles.add}
            aria-label={t("collections.add")}
            title={t("collections.add")}
            onClick={openAddMenu}
          >
            +
          </button>
        )
      }
    >
      {content()}
      {request?.kind === "create" && onChange !== undefined && (
        <CollectionNameDialog
          title={
            request.collectionKind === "folder"
              ? t("collections.newFolderTitle")
              : t("collections.newCollectionTitle")
          }
          initialName=""
          selectedCount={
            request.collectionKind === "collection" ? selectedIds.length : null
          }
          onSubmit={async (name, includeSelected) => {
            const result = await onChange({
              kind: "create",
              parentId: request.parent?.id ?? null,
              collectionKind: request.collectionKind,
              name,
              mediaIds: includeSelected ? [...selectedIds] : [],
            });
            if (result.refusal === null) setRequest(null);
            return result.refusal;
          }}
          onClose={() => {
            setRequest(null);
          }}
        />
      )}
      {request?.kind === "createSmart" && onChange !== undefined && (
        <SmartCollectionDialog
          title={t("collections.newSmartTitle")}
          name=""
          rules={newSmartRules()}
          keywords={keywords}
          folders={folders}
          refines={
            (request.parent?.rules ?? null) === null
              ? null
              : (request.parent?.name ?? null)
          }
          onSubmit={async (name, rules) => {
            const result = await onChange({
              kind: "createSmart",
              parentId: request.parent?.id ?? null,
              name,
              rules,
            });
            if (result.refusal === null) setRequest(null);
            return result.refusal;
          }}
          onClose={() => {
            setRequest(null);
          }}
        />
      )}
      {request?.kind === "editRules" &&
        request.collection.rules !== null &&
        onChange !== undefined && (
          <SmartCollectionDialog
            title={t("collections.editRulesTitle", {
              name: request.collection.name,
            })}
            name={null}
            rules={request.collection.rules}
            keywords={keywords}
            folders={folders}
            refines={smartParentName(request.collection)}
            onSubmit={async (_name, rules) => {
              const result = await onChange({
                kind: "setRules",
                collectionId: request.collection.id,
                rules,
              });
              if (result.refusal === null) setRequest(null);
              return result.refusal;
            }}
            onClose={() => {
              setRequest(null);
            }}
          />
        )}
      {request?.kind === "rename" && onChange !== undefined && (
        <CollectionNameDialog
          title={t("collections.renameTitle", {
            name: request.collection.name,
          })}
          initialName={request.collection.name}
          selectedCount={null}
          onSubmit={async (name) => {
            const result = await onChange({
              kind: "rename",
              collectionId: request.collection.id,
              name,
            });
            if (result.refusal === null) setRequest(null);
            return result.refusal;
          }}
          onClose={() => {
            setRequest(null);
          }}
        />
      )}
      {request?.kind === "deleteFolder" && onChange !== undefined && (
        <DeleteCollectionFolderDialog
          collectionId={request.collection.id}
          name={request.collection.name}
          onConfirm={() => {
            void onChange({
              kind: "delete",
              collectionId: request.collection.id,
            });
            setRequest(null);
          }}
          onClose={() => {
            setRequest(null);
          }}
        />
      )}
    </PanelSection>
  );
}
