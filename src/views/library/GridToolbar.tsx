import { useId } from "react";
import { useTranslation } from "react-i18next";

import { counted, formatCount } from "../../i18n/format";
import type { GridSort, GridSortKey } from "../../services/library";
import { SORT_KEYS } from "../../state/gridFilter";
import styles from "./LibraryView.module.css";

/** Taille des vignettes : bornes et pas du curseur, en pixels. */
export const CELL_SIZE = { min: 80, max: 320, step: 10, default: 160 };

/**
 * Barre sous la grille : nombre d'éléments (affichés sur le total si un
 * filtre en masque, et sélectionnés), tri et taille des vignettes.
 *
 * @param itemCount nombre d'éléments de la source, sans filtre.
 * @param shownCount nombre d'éléments affichés si un filtre est actif.
 */
export function GridToolbar({
  itemCount,
  shownCount,
  selectedCount,
  sort,
  onSortChange,
  cellSize,
  onCellSizeChange,
}: {
  itemCount: number;
  shownCount?: number;
  selectedCount: number;
  sort: GridSort;
  onSortChange: (sort: GridSort) => void;
  cellSize: number;
  onCellSizeChange: (size: number) => void;
}) {
  const { t } = useTranslation();
  const sliderId = useId();
  const sortId = useId();

  const count = () => {
    if (selectedCount > 0) {
      return t("library.selectedCount", {
        ...counted(selectedCount),
        total: formatCount(shownCount ?? itemCount),
      });
    }
    return shownCount === undefined
      ? t("library.itemCount", counted(itemCount))
      : t("library.filteredCount", {
          ...counted(shownCount),
          total: formatCount(itemCount),
        });
  };

  return (
    <footer className={styles.toolbar}>
      <span>{count()}</span>
      <span className={styles.sortControl}>
        <label htmlFor={sortId}>{t("library.sort.label")}</label>
        <select
          id={sortId}
          value={sort.key}
          onChange={(event) => {
            onSortChange({
              ...sort,
              key: event.currentTarget.value as GridSortKey,
            });
          }}
        >
          {SORT_KEYS.map((key) => (
            <option key={key} value={key}>
              {t(`library.sort.${key}`)}
            </option>
          ))}
        </select>
        <button
          type="button"
          className={styles.sortDirection}
          aria-label={
            sort.descending
              ? t("library.sort.descending")
              : t("library.sort.ascending")
          }
          title={
            sort.descending
              ? t("library.sort.descending")
              : t("library.sort.ascending")
          }
          onClick={() => {
            onSortChange({ ...sort, descending: !sort.descending });
          }}
        >
          {sort.descending ? "↓" : "↑"}
        </button>
      </span>
      <span className={styles.sizeControl}>
        <label htmlFor={sliderId}>{t("library.thumbnailSize")}</label>
        <input
          id={sliderId}
          type="range"
          min={CELL_SIZE.min}
          max={CELL_SIZE.max}
          step={CELL_SIZE.step}
          value={cellSize}
          onChange={(event) => {
            onCellSizeChange(event.currentTarget.valueAsNumber);
          }}
        />
      </span>
    </footer>
  );
}
