/**
 * Lignes d'une colonne du filtre Métadonnées : les valeurs reçues du cœur,
 * mises en forme selon la langue, et la date en arbre (année > mois > jour).
 */
import type { TFunction } from "i18next";

import { formatCount } from "../../i18n/format";
import type {
  GridColumnValue,
  GridMetadataColumn,
} from "../../services/metadataBrowser";
import type { TreeItem, TreeNode } from "../../utils/tree";
import { searchKey } from "../../utils/tree";

/** Ligne d'une colonne : une valeur, ou « Tout » (`value` absent). */
export interface ColumnItem extends TreeItem {
  /** Clé de la valeur ; `null` : « Inconnu » ; `undefined` : « Tout ». */
  value: string | null | undefined;
  /** Nombre de médias ; `null` s'il n'a pas de sens (« Tout » des mots-clés). */
  count: number | null;
  /** Précision affichée en plus petit (parent d'un mot-clé). */
  detail: string | null;
}

/** Nom affiché d'une valeur, selon le critère de la colonne. */
export function valueLabel(
  t: TFunction,
  column: GridMetadataColumn,
  value: GridColumnValue,
  language: string,
): string {
  const key = value.value;
  if (key === null) return t("filters.metadata.unknown");
  const number = Number(key);
  const decimal = (digits: number) =>
    new Intl.NumberFormat(language, {
      minimumFractionDigits: digits,
      maximumFractionDigits: digits,
    }).format(number);
  switch (column) {
    case "keyword":
      return value.label ?? key;
    case "colorLabel":
    case "flash":
    case "orientation":
      return t(`filters.metadata.values.${key}` as "filters.metadata.unknown");
    case "fileType":
      return key.toUpperCase();
    case "iso":
      return `ISO ${formatCount(number, language)}`;
    case "aperture":
      return `f/${decimal(1)}`;
    case "shutterSpeed":
      return number >= 1 || number <= 0
        ? `${new Intl.NumberFormat(language).format(number)} s`
        : `1/${formatCount(Math.round(1 / number), language)} s`;
    case "focalLength":
      return `${formatCount(number, language)} mm`;
    case "dimensions":
      return key.replace("×", " × ");
    default:
      return key;
  }
}

/** Ligne « Tout », en tête de chaque colonne. */
function allItem(t: TFunction, count: number | null): ColumnItem {
  return {
    id: 0,
    parentId: null,
    name: t("filters.metadata.all"),
    value: undefined,
    count,
    detail: null,
  };
}

/** Nœud sans enfant, dans l'ordre reçu (et non trié par nom). */
function leaf(item: ColumnItem): TreeNode<ColumnItem> {
  return { item, children: [], key: searchKey(item.name) };
}

/**
 * Arbre d'une colonne : « Tout », puis les valeurs dans l'ordre du cœur. La
 * date devient un arbre année > mois > jour, les plus récents d'abord.
 */
export function columnTree(
  t: TFunction,
  column: GridMetadataColumn,
  values: readonly GridColumnValue[],
  language: string,
): TreeNode<ColumnItem>[] {
  const total =
    column === "keyword"
      ? null
      : values.reduce((sum, value) => sum + value.count, 0);
  let nextId = 1;
  const item = (
    name: string,
    value: string | null,
    count: number,
    detail: string | null = null,
    parentId: number | null = null,
  ): ColumnItem => ({ id: nextId++, parentId, name, value, count, detail });

  if (column !== "date") {
    return [
      leaf(allItem(t, total)),
      ...values.map((value) =>
        leaf(
          item(
            valueLabel(t, column, value, language),
            value.value,
            value.count,
            value.detail,
          ),
        ),
      ),
    ];
  }

  const years: TreeNode<ColumnItem>[] = [];
  const unknown: TreeNode<ColumnItem>[] = [];
  const month = new Intl.DateTimeFormat(language, {
    month: "long",
    timeZone: "UTC",
  });
  const day = new Intl.DateTimeFormat(language, {
    day: "numeric",
    month: "long",
    timeZone: "UTC",
  });
  for (const value of values) {
    const key = value.value;
    if (key === null) {
      unknown.push(
        leaf(item(t("filters.metadata.unknown"), null, value.count)),
      );
      continue;
    }
    const yearKey = key.slice(0, 4);
    const monthKey = key.slice(0, 7);
    let yearNode = years.at(-1);
    if (yearNode?.item.value !== yearKey) {
      yearNode = leaf(item(yearKey, yearKey, 0));
      years.push(yearNode);
    }
    let monthNode = yearNode.children.at(-1);
    if (monthNode?.item.value !== monthKey) {
      monthNode = leaf(
        item(
          formatDate(month, `${monthKey}-01`, monthKey),
          monthKey,
          0,
          null,
          yearNode.item.id,
        ),
      );
      yearNode.children.push(monthNode);
    }
    monthNode.children.push(
      leaf(
        item(
          formatDate(day, key, key),
          key,
          value.count,
          null,
          monthNode.item.id,
        ),
      ),
    );
    monthNode.item.count = (monthNode.item.count ?? 0) + value.count;
    yearNode.item.count = (yearNode.item.count ?? 0) + value.count;
  }
  return [leaf(allItem(t, total)), ...years, ...unknown];
}

/** Date ISO (« 2024-07-14 ») mise en forme ; `fallback` si elle est invalide. */
function formatDate(
  format: Intl.DateTimeFormat,
  isoDate: string,
  fallback: string,
): string {
  const date = new Date(`${isoDate}T00:00:00Z`);
  return Number.isNaN(date.getTime()) ? fallback : format.format(date);
}
