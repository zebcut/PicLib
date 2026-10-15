import { useEffect, useMemo, useState } from "react";
import type { MouseEvent } from "react";
import { useTranslation } from "react-i18next";

import { ResizeHandle } from "../../components/ResizeHandle";
import { Tree } from "../../components/Tree";
import { formatCount } from "../../i18n/format";
import type { GridFilter, GridSource } from "../../services/library";
import { fetchMetadataColumn } from "../../services/metadataBrowser";
import type {
  GridColumnValue,
  GridMetadataColumn,
} from "../../services/metadataBrowser";
import { isApplePlatform } from "../../shortcuts/registry";
import {
  choicesBefore,
  chooseColumnValues,
  columnValues,
  MAX_METADATA_COLUMNS,
  METADATA_COLUMNS,
  toggleIn,
} from "../../state/gridFilter";
import { useStoredNumber } from "../../state/useStoredNumber";
import { visibleRows } from "../../utils/tree";
import styles from "./MetadataBrowser.module.css";
import { columnTree } from "./metadataValues";
import type { ColumnItem } from "./metadataValues";

/** Hauteur du navigateur, en pixels, retenue d'une session à l'autre. */
const HEIGHT = {
  storageKey: "piclib.grid.metadataBrowserHeight",
  default: 200,
  min: 120,
  max: 480,
};

/**
 * Navigateur en colonnes du filtre Métadonnées : chaque
 * colonne liste les valeurs d'un critère avec leur nombre de photos, parmi
 * celles retenues par les autres filtres et par les colonnes à sa gauche.
 * Un clic choisit une valeur, Cmd+clic en ajoute ou en retire une.
 *
 * @param filter critères de la barre de filtres (choix des colonnes
 *   compris).
 * @param countFilter filtre qui s'applique aux nombres : celui de la barre,
 *   ou aucun si les filtres sont désactivés (Cmd+L).
 * @param columns critères des colonnes, dans l'ordre.
 * @param onChange change les critères, à partir des plus récents.
 */
export function MetadataBrowser({
  source,
  filter,
  countFilter,
  columns,
  onColumnsChange,
  refreshKey,
  onChange,
}: {
  source: GridSource;
  filter: GridFilter;
  countFilter: GridFilter;
  columns: readonly GridMetadataColumn[];
  onColumnsChange: (columns: readonly GridMetadataColumn[]) => void;
  refreshKey: number;
  onChange: (update: (filter: GridFilter) => GridFilter) => void;
}) {
  const { t } = useTranslation();
  const setColumns = onColumnsChange;
  const [height, setHeight] = useStoredNumber(
    HEIGHT.storageKey,
    HEIGHT.default,
    HEIGHT.min,
    HEIGHT.max,
  );

  /** Remplace le critère de la colonne `index` ; ses choix sont effacés. */
  const changeColumn = (index: number, column: GridMetadataColumn) => {
    const next = columns.map((current, position) =>
      position === index ? column : current,
    );
    setColumns(next);
    onChange((current) => chooseColumnValues(current, next, index, []));
  };
  const removeColumn = (index: number) => {
    const next = columns.filter((_, position) => position !== index);
    setColumns(next);
    onChange((current) => chooseColumnValues(current, next, index, []));
  };
  const addColumn = () => {
    const unused =
      METADATA_COLUMNS.find((column) => !columns.includes(column)) ?? "date";
    setColumns([...columns, unused]);
  };

  return (
    <div className={styles.browser}>
      <div className={styles.columns} style={{ height }}>
        {columns.map((column, index) => (
          <MetadataColumnList
            // Le critère fait partie de la clé : une autre colonne repart
            // de rien (dépliage de la date compris).
            key={`${String(index)}-${column}`}
            source={source}
            filter={{
              ...countFilter,
              metadata: choicesBefore(countFilter, columns, index),
            }}
            column={column}
            selected={columnValues(filter, columns, index)}
            refreshKey={refreshKey}
            onSelect={(values) => {
              onChange((current) =>
                chooseColumnValues(current, columns, index, values),
              );
            }}
            onColumnChange={(next) => {
              changeColumn(index, next);
            }}
            onRemove={
              columns.length > 1
                ? () => {
                    removeColumn(index);
                  }
                : undefined
            }
          />
        ))}
        {columns.length < MAX_METADATA_COLUMNS && (
          <button
            type="button"
            className={styles.add}
            aria-label={t("filters.metadata.addColumn")}
            title={t("filters.metadata.addColumn")}
            onClick={addColumn}
          >
            +
          </button>
        )}
      </div>
      <ResizeHandle
        label={t("filters.metadata.resize")}
        orientation="horizontal"
        direction={1}
        value={height}
        min={HEIGHT.min}
        max={HEIGHT.max}
        defaultValue={HEIGHT.default}
        onChange={setHeight}
      />
    </div>
  );
}

type Loaded =
  | { status: "loading" }
  | { status: "ready"; values: GridColumnValue[] }
  | { status: "error" };

/** Une colonne : son critère en en-tête, puis ses valeurs. */
function MetadataColumnList({
  source,
  filter,
  column,
  selected,
  refreshKey,
  onSelect,
  onColumnChange,
  onRemove,
}: {
  source: GridSource;
  filter: GridFilter;
  column: GridMetadataColumn;
  selected: readonly (string | null)[];
  refreshKey: number;
  onSelect: (values: (string | null)[]) => void;
  onColumnChange: (column: GridMetadataColumn) => void;
  onRemove: (() => void) | undefined;
}) {
  const { t, i18n } = useTranslation();
  const [loaded, setLoaded] = useState<Loaded>({ status: "loading" });
  const [expanded, setExpanded] = useState<ReadonlySet<string>>(new Set());
  const [scrollElement, setScrollElement] = useState<HTMLDivElement | null>(
    null,
  );
  // Un nouvel objet filtre à chaque affichage : la colonne n'est relue que
  // si son contenu change.
  const filterKey = JSON.stringify(filter);
  const stableFilter = useMemo(
    () => JSON.parse(filterKey) as GridFilter,
    [filterKey],
  );

  useEffect(() => {
    let isActive = true;
    fetchMetadataColumn(source, stableFilter, column).then(
      (values) => {
        if (isActive) setLoaded({ status: "ready", values });
      },
      (error: unknown) => {
        console.error("Failed to read a metadata column", error);
        if (isActive) setLoaded({ status: "error" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [source, stableFilter, column, refreshKey]);

  const roots = useMemo(
    () =>
      loaded.status === "ready"
        ? columnTree(t, column, loaded.values, i18n.language)
        : [],
    [loaded, t, column, i18n.language],
  );
  const itemsById = useMemo(() => {
    const byId = new Map<number, ColumnItem>();
    const visit = (nodes: typeof roots) => {
      for (const node of nodes) {
        byId.set(node.item.id, node.item);
        visit(node.children);
      }
    };
    visit(roots);
    return byId;
  }, [roots]);
  const rows = visibleRows(roots, (id) => {
    const value = itemsById.get(id)?.value;
    return typeof value === "string" && expanded.has(value);
  });

  const isApple = isApplePlatform();
  const choose = (item: ColumnItem, event?: MouseEvent) => {
    if (item.value === undefined) {
      onSelect([]);
      return;
    }
    const combine =
      event !== undefined && (isApple ? event.metaKey : event.ctrlKey);
    onSelect(combine ? toggleIn(selected, item.value) : [item.value]);
  };
  const isSelected = (item: ColumnItem) =>
    item.value === undefined
      ? selected.length === 0
      : selected.includes(item.value);

  return (
    <section
      className={styles.column}
      aria-label={t(`filters.metadata.columns.${column}`)}
    >
      <header className={styles.header}>
        <select
          className={styles.criterion}
          aria-label={t("filters.metadata.criterion")}
          value={column}
          onChange={(event) => {
            onColumnChange(event.currentTarget.value as GridMetadataColumn);
          }}
        >
          {METADATA_COLUMNS.map((option) => (
            <option key={option} value={option}>
              {t(`filters.metadata.columns.${option}`)}
            </option>
          ))}
        </select>
        {onRemove !== undefined && (
          <button
            type="button"
            className={styles.remove}
            aria-label={t("filters.metadata.removeColumn")}
            title={t("filters.metadata.removeColumn")}
            onClick={onRemove}
          >
            −
          </button>
        )}
      </header>
      <div className={styles.values} ref={setScrollElement}>
        {loaded.status === "error" && (
          <p role="alert" className={styles.error}>
            {t("filters.metadata.loadFailed")}
          </p>
        )}
        {loaded.status === "ready" && (
          <Tree
            label={t(`filters.metadata.columns.${column}`)}
            rows={rows}
            scrollElement={scrollElement}
            onToggle={(ids, isExpanded) => {
              const values = ids
                .map((id) => itemsById.get(id)?.value)
                .filter((value) => typeof value === "string");
              setExpanded((current) => {
                const next = new Set(current);
                for (const value of values) {
                  if (isExpanded) next.add(value);
                  else next.delete(value);
                }
                return next;
              });
            }}
            onActivate={(item) => {
              choose(item);
            }}
            rowProps={(row) => ({
              "aria-selected": isSelected(row.node.item),
              className: isSelected(row.node.item)
                ? styles.selected
                : undefined,
              onClick: (event) => {
                choose(row.node.item, event);
              },
            })}
            renderRow={(row) => (
              <span className={styles.row}>
                <span className={styles.name}>
                  {row.node.item.name}
                  {row.node.item.detail !== null && (
                    <span className={styles.detail}>
                      {" "}
                      {row.node.item.detail}
                    </span>
                  )}
                </span>
                {row.node.item.count !== null && (
                  <span className={styles.count}>
                    {formatCount(row.node.item.count)}
                  </span>
                )}
              </span>
            )}
          />
        )}
      </div>
    </section>
  );
}
