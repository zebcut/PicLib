/**
 * Barre de filtres de la grille et tri :
 * fonctions pures, chaque opération renvoie un nouvel état.
 */
import type {
  GridFilter,
  GridSort,
  GridSortKey,
  GridTextSearch,
} from "../services/library";
import type {
  GridColumnChoice,
  GridMetadataColumn,
} from "../services/metadataBrowser";

/** Filtre qui laisse tout passer. */
export const NO_FILTER: GridFilter = {
  flags: [],
  rating: null,
  colors: [],
  kinds: [],
  text: null,
  metadata: [],
};

/** Filtre Texte à l'ouverture de son onglet : rien de saisi. */
export const EMPTY_TEXT_SEARCH: GridTextSearch = {
  field: "all",
  rule: "contains",
  text: "",
};

/**
 * Vrai si la saisie du filtre Texte contient au moins un terme (séparés par
 * des virgules ou des espaces, « ! » devant un terme l'exclut), comme le
 * découpe le cœur.
 */
export function hasTerms(text: string): boolean {
  return text.split(/[\s,]+/).some((term) => term.replace(/^!/, "") !== "");
}

/** Tri par défaut : les plus récentes d'abord. */
export const DEFAULT_SORT: GridSort = { key: "captureTime", descending: true };

/** Critères de tri, dans l'ordre du menu. */
export const SORT_KEYS: readonly GridSortKey[] = [
  "captureTime",
  "importTime",
  "fileName",
  "rating",
  "flag",
  "colorLabel",
  "fileType",
];

/** Vrai si le filtre écarte des éléments (au moins un critère). */
export function isFiltering(filter: GridFilter): boolean {
  return (
    filter.flags.length > 0 ||
    filter.rating !== null ||
    filter.colors.length > 0 ||
    filter.kinds.length > 0 ||
    (filter.text !== null && hasTerms(filter.text.text)) ||
    filter.metadata.some((choice) => choice.values.length > 0) ||
    (filter.area !== undefined && filter.area !== null)
  );
}

/** Onglets de la barre de filtres. */
export type FilterTab = "text" | "attribute" | "metadata";

/** État de la barre de filtres. */
export interface FilterBarState {
  /** Onglets ouverts ; Cmd+clic en ouvre plusieurs. */
  readonly tabs: readonly FilterTab[];
  /** Critères des filtres Texte, Attribut et Métadonnées. */
  readonly filter: GridFilter;
  /** Faux après Cmd+L : les critères sont gardés mais n'agissent plus. */
  readonly enabled: boolean;
}

/** Aucun filtre : l'onglet « Aucun ». */
export const NO_FILTER_BAR: FilterBarState = {
  tabs: [],
  filter: NO_FILTER,
  enabled: true,
};

/**
 * Clic sur un onglet : il devient le seul ouvert, ou se ferme s'il l'était
 * déjà seul ; avec Cmd (`combine`), il s'ajoute aux autres ou s'en retire.
 * Un onglet fermé perd ses critères ; ouvrir un onglet réactive les
 * filtres.
 */
export function chooseTab(
  state: FilterBarState,
  tab: FilterTab,
  combine: boolean,
): FilterBarState {
  const isOpen = state.tabs.includes(tab);
  const tabs = combine
    ? toggleIn(state.tabs, tab)
    : isOpen && state.tabs.length === 1
      ? []
      : [tab];
  const filter = state.filter;
  return {
    tabs,
    filter: {
      ...(tabs.includes("attribute")
        ? filter
        : { ...NO_FILTER, text: filter.text, metadata: filter.metadata }),
      text: tabs.includes("text") ? (filter.text ?? EMPTY_TEXT_SEARCH) : null,
      metadata: tabs.includes("metadata") ? filter.metadata : [],
    },
    enabled: true,
  };
}

/** Change les critères des filtres, et réactive les filtres. */
export function changeFilter(
  state: FilterBarState,
  filter: GridFilter,
): FilterBarState {
  return { ...state, filter, enabled: true };
}

/** Active ou désactive tous les filtres (Cmd+L), sans perdre les critères. */
export function toggleEnabled(state: FilterBarState): FilterBarState {
  return { ...state, enabled: !state.enabled };
}

/** Filtre à appliquer à la grille. */
export function activeFilter(state: FilterBarState): GridFilter {
  return state.enabled ? state.filter : NO_FILTER;
}

/** Ajoute `value` à la liste, ou l'en retire s'il y était. */
export function toggleIn<T>(list: readonly T[], value: T): T[] {
  return list.includes(value)
    ? list.filter((item) => item !== value)
    : [...list, value];
}

/** Critères proposés dans l'en-tête des colonnes du filtre Métadonnées. */
export const METADATA_COLUMNS: readonly GridMetadataColumn[] = [
  "date",
  "camera",
  "lens",
  "colorLabel",
  "keyword",
  "country",
  "region",
  "city",
  "location",
  "fileType",
  "iso",
  "aperture",
  "shutterSpeed",
  "focalLength",
  "flash",
  "creator",
  "dimensions",
  "orientation",
];

/** Colonnes par défaut du filtre Métadonnées. */
export const DEFAULT_METADATA_COLUMNS: readonly GridMetadataColumn[] = [
  "date",
  "camera",
  "lens",
  "colorLabel",
];

/** Nombre de colonnes du filtre Métadonnées, au plus. */
export const MAX_METADATA_COLUMNS = 8;

/**
 * Valeurs choisies dans la colonne `index` ; aucune si ses choix étaient
 * ceux d'un autre critère (colonne changée entre-temps).
 */
export function columnValues(
  filter: GridFilter,
  columns: readonly GridMetadataColumn[],
  index: number,
): readonly (string | null)[] {
  const choice = filter.metadata[index];
  return choice !== undefined && choice.column === columns[index]
    ? choice.values
    : [];
}

/**
 * Choix des colonnes avant `index` : ceux qui affinent la colonne `index`
 * (les colonnes s'affinent de gauche à droite).
 */
export function choicesBefore(
  filter: GridFilter,
  columns: readonly GridMetadataColumn[],
  index: number,
): GridColumnChoice[] {
  return columns.slice(0, index).map((column, position) => ({
    column,
    values: [...columnValues(filter, columns, position)],
  }));
}

/**
 * Choisit `values` dans la colonne `index` : les choix des colonnes à sa
 * droite sont effacés, ils pourraient ne plus exister.
 */
export function chooseColumnValues(
  filter: GridFilter,
  columns: readonly GridMetadataColumn[],
  index: number,
  values: readonly (string | null)[],
): GridFilter {
  return {
    ...filter,
    metadata: [
      ...choicesBefore(filter, columns, index),
      ...(columns[index] === undefined
        ? []
        : [{ column: columns[index], values: [...values] }]),
    ],
  };
}
