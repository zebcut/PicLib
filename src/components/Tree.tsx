import { defaultRangeExtractor, useVirtualizer } from "@tanstack/react-virtual";
import { useCallback, useLayoutEffect, useRef, useState } from "react";
import type {
  CSSProperties,
  HTMLAttributes,
  KeyboardEvent,
  MouseEvent,
  ReactNode,
} from "react";

import { branchIds } from "../utils/tree";
import type { TreeItem, TreeRow } from "../utils/tree";
import { DisclosureTriangle } from "./DisclosureTriangle";
import styles from "./Tree.module.css";

/** Décalage d'un niveau à l'autre, en pixels. */
export const TREE_INDENT_PX = 14;

/**
 * Hauteur d'une ligne, en pixels : fixe, pour savoir sans les dessiner où
 * se trouvent les lignes.
 */
export const TREE_ROW_HEIGHT_PX = 22;

/** Lignes dessinées en plus, au-dessus et au-dessous de la partie visible. */
const OVERSCAN_ROWS = 10;

/** Attribut du triangle : un appui dessus n'est pas le début d'un glisser. */
export const TREE_TWISTY_ATTRIBUTE = "data-tree-twisty";

/**
 * Attributs supplémentaires d'une ligne : une classe (ajoutée à celle de la
 * ligne), des gestionnaires de pointeur et de clic, l'état « choisie »
 * (dossier affiché), des attributs `data-` (cible de dépôt, par exemple).
 */
export type TreeRowProps = Pick<
  HTMLAttributes<HTMLLIElement>,
  "className" | "onPointerDown" | "onClick" | "aria-selected"
> &
  Partial<Record<`data-${string}`, string>>;

/**
 * Arbre repliable affiché en liste (mots-clés), avec un triangle devant
 * chaque branche. Au clavier, une seule ligne reçoit le focus à la fois :
 * ↑ ↓ passent d'une ligne à l'autre, → déplie ou descend au premier
 * enfant, ← replie ou remonte au parent, Début et Fin vont aux extrémités,
 * Espace « active » la ligne (`onActivate`), Suppr et ⌫ la suppriment
 * (`onDelete`). Option+clic sur un triangle,
 * ou Option+→ et Option+←, déplient ou replient toute la branche.
 *
 * Avec `scrollElement` (la zone qui défile autour de l'arbre, `null` tant
 * qu'elle n'est pas affichée), seules les lignes visibles sont dessinées :
 * l'arbre reste fluide avec des dizaines de milliers de lignes. Sans elle,
 * toutes les lignes sont dessinées (petits arbres).
 *
 * @param rows lignes visibles (voir `visibleRows`).
 * @param renderRow contenu d'une ligne, après le triangle.
 * @param onToggle déplie (`true`) ou replie les branches indiquées : une
 *   seule, ou toute une branche avec Option.
 * @param contentWidth largeur minimale des lignes, en pixels : au-delà de
 *   la zone visible, elle défile horizontalement.
 */
export function Tree<T extends TreeItem>({
  label,
  rows,
  renderRow,
  onToggle,
  onRowContextMenu,
  onActivate,
  onDelete,
  rowProps,
  scrollElement,
  contentWidth,
}: {
  label: string;
  rows: readonly TreeRow<T>[];
  renderRow: (row: TreeRow<T>) => ReactNode;
  onToggle: (ids: number[], expanded: boolean) => void;
  onRowContextMenu?: (item: T, event: MouseEvent) => void;
  /** Espace sur une ligne (cocher sa case, par exemple). */
  onActivate?: (item: T) => void;
  /** Suppr ou ⌫ sur une ligne (supprimer un dossier, par exemple). */
  onDelete?: (item: T) => void;
  rowProps?: (row: TreeRow<T>) => TreeRowProps;
  /**
   * L'élément lui-même, et non une référence : l'arbre doit être redessiné
   * dès qu'il est affiché (il l'est après l'arbre, étant son parent).
   */
  scrollElement?: HTMLElement | null;
  contentWidth?: number;
}) {
  const [focusedId, setFocusedId] = useState<number | null>(null);
  const rowElements = useRef(new Map<number, HTMLLIElement>());
  // Ligne à qui donner le focus dès qu'elle sera dessinée.
  const pendingFocus = useRef<number | null>(null);
  const focusedIndex = rows.findIndex((row) => row.node.item.id === focusedId);
  const isVirtual = scrollElement !== undefined;

  // La ligne qui a le focus reste dessinée même hors de la partie visible :
  // le clavier continue d'agir sur elle.
  const rangeExtractor = useCallback(
    (range: Parameters<typeof defaultRangeExtractor>[0]) => {
      const indexes = defaultRangeExtractor(range);
      if (focusedIndex >= 0 && !indexes.includes(focusedIndex)) {
        indexes.push(focusedIndex);
        indexes.sort((a, b) => a - b);
      }
      return indexes;
    },
    [focusedIndex],
  );
  // Avertissement propre au compilateur React, que PicLib n'utilise pas : les
  // fonctions renvoyées par useVirtualizer ne sont transmises à aucun
  // composant mémorisé.
  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rows.length,
    getScrollElement: () => scrollElement ?? null,
    estimateSize: () => TREE_ROW_HEIGHT_PX,
    overscan: OVERSCAN_ROWS,
    rangeExtractor,
    enabled: isVirtual,
    // Avant la première mesure de la zone qui défile : assez de lignes pour
    // la remplir, sans instant de liste vide.
    initialRect: { width: 0, height: window.innerHeight },
  });

  const drawn: { row: TreeRow<T>; style?: CSSProperties }[] = isVirtual
    ? virtualizer.getVirtualItems().flatMap((virtualRow) => {
        const row = rows[virtualRow.index];
        return row === undefined
          ? []
          : [
              {
                row,
                style: {
                  position: "absolute",
                  top: 0,
                  left: 0,
                  width: "100%",
                  height: TREE_ROW_HEIGHT_PX,
                  transform: `translateY(${String(virtualRow.start)}px)`,
                },
              },
            ];
      })
    : rows.map((row) => ({ row }));

  // La ligne qui reçoit Tab : celle qui a le focus si elle est dessinée,
  // sinon la première dessinée.
  const tabStop = drawn.some(({ row }) => row.node.item.id === focusedId)
    ? focusedId
    : (drawn[0]?.row.node.item.id ?? null);

  useLayoutEffect(() => {
    const id = pendingFocus.current;
    if (id === null) return;
    const element = rowElements.current.get(id);
    if (element === undefined) return;
    pendingFocus.current = null;
    element.focus();
  });

  const focus = (index: number) => {
    const id = rows[index]?.node.item.id;
    if (id === undefined) return;
    setFocusedId(id);
    if (isVirtual) virtualizer.scrollToIndex(index, { align: "auto" });
    const element = rowElements.current.get(id);
    if (element === undefined) pendingFocus.current = id;
    else element.focus();
  };

  const toggle = (row: TreeRow<T>, expanded: boolean, wholeBranch: boolean) => {
    onToggle(wholeBranch ? branchIds(row.node) : [row.node.item.id], expanded);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLUListElement>) => {
    // Les touches tapées dans un champ ou un bouton de la ligne restent à
    // ce champ.
    const target = event.target as HTMLElement;
    if (target.getAttribute("role") !== "treeitem") return;
    const index = rows.findIndex(
      (row) => rowElements.current.get(row.node.item.id) === target,
    );
    const row = rows[index];
    if (row === undefined) return;
    switch (event.key) {
      case "ArrowDown":
        focus(index + 1);
        break;
      case "ArrowUp":
        focus(index - 1);
        break;
      case "Home":
        focus(0);
        break;
      case "End":
        focus(rows.length - 1);
        break;
      case "ArrowRight":
        if (!row.hasChildren) return;
        if (event.altKey) toggle(row, true, true);
        else if (row.isExpanded) focus(index + 1);
        else toggle(row, true, false);
        break;
      case "ArrowLeft":
        if (row.hasChildren && (row.isExpanded || event.altKey)) {
          toggle(row, false, event.altKey);
        } else {
          const parent = rows.findIndex(
            (candidate) => candidate.node.item.id === row.parentId,
          );
          if (parent >= 0) focus(parent);
        }
        break;
      case " ":
        if (onActivate === undefined) return;
        onActivate(row.node.item);
        break;
      case "Delete":
      case "Backspace":
        if (onDelete === undefined) return;
        onDelete(row.node.item);
        break;
      default:
        return;
    }
    event.preventDefault();
  };

  return (
    <ul
      className={styles.tree}
      role="tree"
      aria-label={label}
      onKeyDown={onKeyDown}
      style={{
        ...(isVirtual && {
          position: "relative",
          height: virtualizer.getTotalSize(),
        }),
        ...(contentWidth !== undefined && { minWidth: contentWidth }),
      }}
    >
      {drawn.map(({ row, style }) => {
        const { item } = row.node;
        const { className: extraClass, ...extra } = rowProps?.(row) ?? {};
        return (
          <li
            {...extra}
            key={item.id}
            ref={(element) => {
              if (element === null) rowElements.current.delete(item.id);
              else rowElements.current.set(item.id, element);
            }}
            role="treeitem"
            aria-level={row.depth + 1}
            aria-posinset={row.position}
            aria-setsize={row.siblingCount}
            aria-expanded={row.hasChildren ? row.isExpanded : undefined}
            tabIndex={item.id === tabStop ? 0 : -1}
            className={[styles.row, extraClass].join(" ").trim()}
            style={{ ...style, paddingLeft: row.depth * TREE_INDENT_PX }}
            onFocus={(event) => {
              if (event.target === event.currentTarget) setFocusedId(item.id);
            }}
            onContextMenu={
              onRowContextMenu === undefined
                ? undefined
                : (event) => {
                    event.preventDefault();
                    onRowContextMenu(item, event);
                  }
            }
          >
            {row.hasChildren ? (
              <span
                className={styles.twisty}
                aria-hidden
                {...{ [TREE_TWISTY_ATTRIBUTE]: "" }}
                onClick={(event) => {
                  toggle(row, !row.isExpanded, event.altKey);
                }}
              >
                <DisclosureTriangle expanded={row.isExpanded} />
              </span>
            ) : (
              <span className={styles.twisty} aria-hidden />
            )}
            {renderRow(row)}
          </li>
        );
      })}
    </ul>
  );
}
