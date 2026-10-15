import { useDeferredValue, useMemo, useState } from "react";
import type { MouseEvent } from "react";
import { useTranslation } from "react-i18next";

import { PanelSection } from "../../components/PanelSection";
import { ResizeHandle } from "../../components/ResizeHandle";
import {
  TREE_INDENT_PX,
  TREE_ROW_HEIGHT_PX,
  Tree,
} from "../../components/Tree";
import { formatCount } from "../../i18n/format";
import { createKeyword } from "../../services/keywordList";
import type {
  KeywordListItemDto,
  KeywordListOperation,
  KeywordListRefusal,
} from "../../services/keywordList";
import type { KeywordPresenceDto } from "../../services/keywords";
import type { KeywordList } from "../../state/useKeywordList";
import { selectionPresence } from "../../state/useKeywordPanel";
import type { KeywordPanelState } from "../../state/useKeywordPanel";
import { useStoredIdSet } from "../../state/useStoredIdSet";
import { useStoredNumber } from "../../state/useStoredNumber";
import { textWidth, uiFont } from "../../utils/textWidth";
import { buildTree, filterTree, itemPath, visibleRows } from "../../utils/tree";
import { listRefusalText } from "./KeywordDialogs";
import { KeywordListDialog } from "./KeywordListDialog";
import type { KeywordListDialogState } from "./KeywordListDialog";
import { popUpContextMenu } from "../../menu/contextMenu";
import { keywordMenuItems } from "./keywordContextMenu";
import styles from "./KeywordListPanel.module.css";
import { useKeywordListDrag } from "./useKeywordListDrag";

/** Clé sous laquelle l'état replié de la section est retenu. */
const SECTION_STORAGE_KEY = "piclib.panel.keywordList.open";

/** Clé des branches dépliées de l'arbre (repliées par défaut). */
const EXPANDED_STORAGE_KEY = "piclib.panel.keywordList.expanded";

/** Clé de la hauteur de la liste, réglée en glissant son bord bas. */
const HEIGHT_STORAGE_KEY = "piclib.panel.keywordList.height";

/** Hauteur de la liste, en pixels. */
const LIST_HEIGHT = { default: 360, min: 120, max: 2000 };

/**
 * Largeur d'une ligne en plus de son décalage, de son nom et de son
 * nombre de photos : triangle (22), case (13), flèche (20), espaces et
 * marge.
 */
const ROW_EXTRAS_PX = 80;

/** Aucune branche refermée. */
const NONE: ReadonlySet<number> = new Set();

/**
 * Panneau « Liste des mots-clés » : tout l'arbre, avec le
 * nombre de photos de chaque mot-clé (sa branche entière) et une case qui
 * reflète la sélection (cochée : toutes les photos l'ont ; tiret : une
 * partie). Cocher pose le mot-clé sur toute la sélection, décocher le
 * retire. La flèche affiche ses photos dans la grille ; le clic droit
 * crée, renomme, déplace, fusionne, supprime. Un mot-clé se glisse sur un
 * autre pour s'y ranger ; des photos se glissent sur un mot-clé pour le
 * recevoir.
 *
 * Seules les lignes visibles sont dessinées : la liste reste fluide avec
 * des dizaines de milliers de mots-clés. Sa hauteur se règle en glissant
 * son bord bas ; un nom trop long pour la colonne la fait défiler
 * horizontalement.
 */
export function KeywordListPanel({
  list,
  selectedIds,
  selection,
  onAddKeyword,
  onRemoveKeyword,
  onChange,
  onShowKeyword,
  onImportList,
  onExportList,
}: {
  list: KeywordList;
  selectedIds: ReadonlySet<number>;
  /** Mots-clés de la sélection, pour les cases à cocher. */
  selection: KeywordPanelState;
  onAddKeyword: (mediaIds: number[], keywordId: number) => void;
  onRemoveKeyword: (mediaIds: number[], keywordId: number) => void;
  onChange: (
    operation: KeywordListOperation,
  ) => Promise<KeywordListRefusal | null>;
  onShowKeyword: (keywordId: number) => void;
  /** Importe une liste au format texte de Lightroom (clic droit). */
  onImportList?: () => void;
  /** Exporte la liste au format texte de Lightroom (clic droit). */
  onExportList?: () => void;
}) {
  const { t, i18n } = useTranslation();
  const [query, setQuery] = useState("");
  const [expanded, setExpanded] = useStoredIdSet(EXPANDED_STORAGE_KEY);
  const [height, setHeight] = useStoredNumber(
    HEIGHT_STORAGE_KEY,
    LIST_HEIGHT.default,
    LIST_HEIGHT.min,
    LIST_HEIGHT.max,
  );
  // Branches refermées pendant la recherche en cours : une nouvelle
  // recherche rouvre tout ce qu'elle trouve.
  const [searchCollapsed, setSearchCollapsed] = useState<{
    query: string;
    ids: ReadonlySet<number>;
  }>({ query: "", ids: NONE });
  // La zone qui défile, pour ne dessiner que les lignes visibles.
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(
    null,
  );
  const [dialog, setDialog] = useState<KeywordListDialogState | null>(null);
  const [createFailed, setCreateFailed] = useState(false);
  const [moveRefusal, setMoveRefusal] = useState<KeywordListRefusal | null>(
    null,
  );

  const keywords = useMemo(
    () => (list.state.status === "ready" ? list.state.keywords : []),
    [list.state],
  );
  const byId = useMemo(
    () => new Map(keywords.map((keyword) => [keyword.id, keyword])),
    [keywords],
  );
  const tree = useMemo(() => buildTree(keywords), [keywords]);
  // Le champ suit la frappe ; l'arbre filtré suit dès qu'il le peut.
  const search = useDeferredValue(query).trim();
  const searching = search !== "";
  const collapsedInSearch =
    searchCollapsed.query === search ? searchCollapsed.ids : NONE;
  const found = useMemo(() => filterTree(tree, search), [tree, search]);
  const rows = useMemo(
    () =>
      visibleRows(found, (id) =>
        // Pendant une recherche, tout ce qui est trouvé est déplié, sauf ce
        // que l'on a refermé.
        searching ? !collapsedInSearch.has(id) : expanded.has(id),
      ),
    [found, searching, collapsedInSearch, expanded],
  );
  const presence = useMemo(() => selectionPresence(selection), [selection]);

  // Largeur de la ligne la plus longue : au-delà de la colonne, la liste
  // défile horizontalement au lieu de couper les noms.
  const language = i18n.language;
  const contentWidth = useMemo(() => {
    const font = uiFont("--font-size-small", 12);
    let widest = 0;
    for (const { depth, node } of rows) {
      const count =
        node.item.photoCount > 0
          ? formatCount(node.item.photoCount, language)
          : "";
      widest = Math.max(
        widest,
        depth * TREE_INDENT_PX +
          textWidth(node.item.name, font) +
          textWidth(count, font),
      );
    }
    return Math.ceil(widest + ROW_EXTRAS_PX);
  }, [rows, language]);

  /** Déplie ou replie des branches, dans la recherche ou dans l'arbre. */
  const toggleBranches = (ids: number[], open: boolean) => {
    if (!searching) {
      setExpanded(ids, open);
      return;
    }
    setSearchCollapsed((previous) => {
      const next = new Set(previous.query === search ? previous.ids : NONE);
      for (const id of ids) {
        if (open) next.delete(id);
        else next.add(id);
      }
      return { query: search, ids: next };
    });
  };

  const run = (operation: KeywordListOperation) =>
    onChange(operation).then((refusal) => {
      if (refusal === null) setDialog(null);
      return refusal;
    });
  const menuActions = {
    create: (parent: KeywordListItemDto | null) => {
      setDialog({ kind: "create", parent });
    },
    rename: (keyword: KeywordListItemDto) => {
      setDialog({ kind: "rename", keyword });
    },
    move: (keyword: KeywordListItemDto) => {
      setDialog({ kind: "move", keyword });
    },
    merge: (keyword: KeywordListItemDto) => {
      setDialog({ kind: "merge", keyword });
    },
    remove: (keyword: KeywordListItemDto) => {
      setDialog({ kind: "delete", keyword });
    },
    setExport: (keyword: KeywordListItemDto, include: boolean) => {
      void run({ kind: "export", keywordId: keyword.id, include });
    },
    showPhotos: (keyword: KeywordListItemDto) => {
      onShowKeyword(keyword.id);
    },
    deleteUnused: () => {
      setDialog({ kind: "deleteUnused" });
    },
    importList: onImportList,
    exportList: onExportList,
  };
  const openMenu = (keyword: KeywordListItemDto | null) => {
    popUpContextMenu(keywordMenuItems(t, keyword, menuActions)).catch(
      (error: unknown) => {
        console.error("Failed to open the keyword menu", error);
      },
    );
  };

  const drag = useKeywordListDrag(
    keywords,
    expanded,
    {
      move: (keywordId, parentId) => {
        onChange({ kind: "move", keywordId, parentId })
          .then(setMoveRefusal)
          .catch((error: unknown) => {
            console.error("Failed to move the keyword", error);
          });
        if (parentId !== null) setExpanded(parentId, true);
      },
      apply: onAddKeyword,
      expand: (keywordId) => {
        setExpanded(keywordId, true);
      },
    },
    { hovered: styles.dropTarget, dragged: styles.dragged },
  );

  const toggle = (keyword: KeywordListItemDto) => {
    const ids = [...selectedIds];
    if (presence.get(keyword.id) === "all") onRemoveKeyword(ids, keyword.id);
    else onAddKeyword(ids, keyword.id);
  };

  const content = () => {
    switch (list.state.status) {
      case "loading":
        return null;
      case "failed":
        return (
          <p className={styles.message} role="alert">
            {t("keywordList.loadFailed")}
          </p>
        );
      case "ready":
        break;
    }
    return (
      <>
        <div className={styles.toolbar}>
          <input
            className={styles.input}
            type="search"
            spellCheck={false}
            aria-label={t("keywordList.search")}
            placeholder={t("keywordList.search")}
            value={query}
            onChange={(event) => {
              setQuery(event.target.value);
            }}
          />
          <button
            type="button"
            className={styles.iconButton}
            aria-label={t("keywordList.menu.createRoot")}
            title={t("keywordList.menu.createRoot")}
            onClick={() => {
              setDialog({ kind: "create", parent: null });
            }}
          >
            +
          </button>
        </div>
        {createFailed && (
          <p className={styles.message} role="alert">
            {t("keywordList.createFailed")}
          </p>
        )}
        {moveRefusal !== null && (
          <p className={styles.message} role="alert">
            {listRefusalText(t, moveRefusal)}
          </p>
        )}
        {keywords.length === 0 && (
          <p className={styles.message}>{t("keywordList.empty")}</p>
        )}
        {keywords.length > 0 && rows.length === 0 && (
          <p className={styles.message}>{t("keywordList.noMatch")}</p>
        )}
        {rows.length > 0 && (
          <div
            {...drag.rootProps}
            ref={setScrollElement}
            className={
              drag.isRootHovered
                ? [styles.tree, styles.rootTarget].join(" ")
                : styles.tree
            }
            style={{ maxHeight: height }}
          >
            <Tree
              label={t("keywordList.title")}
              rows={rows}
              scrollElement={scrollElement}
              contentWidth={contentWidth}
              rowProps={(row) => drag.rowProps(row.node.item)}
              onToggle={toggleBranches}
              onRowContextMenu={(keyword) => {
                openMenu(keyword);
              }}
              onActivate={(keyword) => {
                if (selectedIds.size > 0) toggle(keyword);
              }}
              renderRow={({ node: { item: keyword } }) => (
                <KeywordRow
                  keyword={keyword}
                  path={itemPath(byId, keyword.id)}
                  presence={presence.get(keyword.id) ?? "absent"}
                  canApply={selectedIds.size > 0}
                  onToggle={() => {
                    toggle(keyword);
                  }}
                  onRename={() => {
                    setDialog({ kind: "rename", keyword });
                  }}
                  onShow={() => {
                    onShowKeyword(keyword.id);
                  }}
                />
              )}
            />
          </div>
        )}
        {rows.length > 0 && (
          <ResizeHandle
            label={t("keywordList.resizeHeight")}
            orientation="horizontal"
            direction={1}
            // La hauteur affichée : la liste peut être plus courte que la
            // hauteur choisie.
            value={Math.max(
              LIST_HEIGHT.min,
              Math.min(height, rows.length * TREE_ROW_HEIGHT_PX),
            )}
            min={LIST_HEIGHT.min}
            max={LIST_HEIGHT.max}
            defaultValue={LIST_HEIGHT.default}
            onChange={setHeight}
          />
        )}
      </>
    );
  };

  return (
    <PanelSection
      title={t("keywordList.title")}
      storageKey={SECTION_STORAGE_KEY}
    >
      <div
        onContextMenu={(event: MouseEvent) => {
          // Un clic droit sur un mot-clé a déjà ouvert son propre menu.
          if (event.defaultPrevented) return;
          event.preventDefault();
          openMenu(null);
        }}
      >
        {content()}
      </div>
      {dialog !== null && (
        <KeywordListDialog
          dialog={dialog}
          keywords={keywords}
          onCreate={async (parent, name, includeOnExport) => {
            try {
              const created = await createKeyword(
                parent?.id ?? null,
                name,
                includeOnExport,
              );
              if (created.status === "refused") return created.refusal;
              setCreateFailed(false);
            } catch (error: unknown) {
              console.error("Failed to create the keyword", error);
              setCreateFailed(true);
            }
            if (parent !== null) setExpanded(parent.id, true);
            list.reload();
            setDialog(null);
            return null;
          }}
          onRun={run}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
    </PanelSection>
  );
}

function KeywordRow({
  keyword,
  path,
  presence,
  canApply,
  onToggle,
  onRename,
  onShow,
}: {
  keyword: KeywordListItemDto;
  /** Chemin complet (« Lieux > France > Paris »), affiché au survol. */
  path: string;
  presence: KeywordPresenceDto;
  canApply: boolean;
  onToggle: () => void;
  onRename: () => void;
  onShow: () => void;
}) {
  const { t, i18n } = useTranslation();
  return (
    <>
      <input
        type="checkbox"
        className={styles.checkbox}
        ref={(element) => {
          if (element !== null) element.indeterminate = presence === "partial";
        }}
        checked={presence === "all"}
        disabled={!canApply}
        tabIndex={-1}
        aria-label={t("keywordList.apply", { name: keyword.name })}
        title={t("keywordList.apply", { name: keyword.name })}
        onChange={onToggle}
      />
      <span
        className={[styles.name, keyword.includeOnExport ? "" : styles.hidden]
          .join(" ")
          .trim()}
        title={
          keyword.includeOnExport
            ? path
            : `${path}\n${t("keywordList.notExported")}`
        }
        onDoubleClick={onRename}
      >
        {keyword.name}
      </span>
      <span className={styles.count}>
        {keyword.photoCount > 0
          ? formatCount(keyword.photoCount, i18n.language)
          : ""}
      </span>
      <button
        type="button"
        className={styles.show}
        tabIndex={-1}
        disabled={keyword.photoCount === 0}
        aria-label={t("keywordList.showPhotos", { name: keyword.name })}
        title={t("keywordList.showPhotos", { name: keyword.name })}
        onClick={onShow}
      >
        →
      </button>
    </>
  );
}
