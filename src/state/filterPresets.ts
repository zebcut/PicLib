/**
 * Préréglages de filtres : ceux fournis par PicLib, et la
 * comparaison d'un préréglage avec l'état de la barre de filtres. Fonctions
 * pures.
 */
import type { GridFilter } from "../services/library";
import type { FilterPresetContent } from "../services/filterPresets";
import type { GridMetadataColumn } from "../services/metadataBrowser";
import { DEFAULT_METADATA_COLUMNS, NO_FILTER } from "./gridFilter";
import type { FilterBarState } from "./gridFilter";

/** Préréglage fourni par PicLib : son nom vient des traductions. */
export interface BuiltInPreset {
  /** Clé de traduction (`filters.presets.builtIn.<key>`). */
  key: string;
  content: FilterPresetContent;
}

const metadata = (columns: readonly GridMetadataColumn[]) => ({
  tabs: ["metadata" as const],
  filter: NO_FILTER,
  columns: [...columns],
});
const attribute = (filter: Partial<GridFilter>) => ({
  tabs: ["attribute" as const],
  filter: { ...NO_FILTER, ...filter },
  columns: [...DEFAULT_METADATA_COLUMNS],
});

/** Préréglages fournis, dans l'ordre du menu. */
export const BUILT_IN_PRESETS: readonly BuiltInPreset[] = [
  {
    key: "off",
    content: {
      tabs: [],
      filter: NO_FILTER,
      columns: [...DEFAULT_METADATA_COLUMNS],
    },
  },
  { key: "picked", content: attribute({ flags: ["picked"] }) },
  {
    key: "rated",
    content: attribute({ rating: { comparison: "atLeast", stars: 1 } }),
  },
  {
    key: "unrated",
    content: attribute({ rating: { comparison: "equal", stars: 0 } }),
  },
  { key: "defaultColumns", content: metadata(DEFAULT_METADATA_COLUMNS) },
  {
    key: "cameraInfo",
    content: metadata([
      "camera",
      "lens",
      "focalLength",
      "aperture",
      "shutterSpeed",
      "iso",
    ]),
  },
  {
    key: "location",
    content: metadata(["country", "region", "city", "location"]),
  },
];

/** Ce qu'un préréglage enregistrerait de l'état actuel de la barre. */
export function presetContent(
  state: FilterBarState,
  columns: readonly GridMetadataColumn[],
): FilterPresetContent {
  return { tabs: [...state.tabs], filter: state.filter, columns: [...columns] };
}

/**
 * Forme comparable d'un préréglage : l'ordre des choix ne compte pas, un
 * critère vide non plus ; les colonnes ne comptent que si l'onglet
 * Métadonnées est ouvert.
 */
export function presetKey(content: FilterPresetContent): string {
  const sorted = (values: readonly (string | null)[]) =>
    values.map(String).sort();
  const { filter } = content;
  const hasMetadata = content.tabs.includes("metadata");
  return JSON.stringify({
    tabs: sorted(content.tabs),
    flags: sorted(filter.flags),
    rating: filter.rating,
    colors: sorted(filter.colors),
    kinds: sorted(filter.kinds),
    text: content.tabs.includes("text") ? filter.text : null,
    metadata: hasMetadata
      ? filter.metadata.flatMap((choice, index) =>
          choice.values.length === 0
            ? []
            : [[index, choice.column, sorted(choice.values)]],
        )
      : [],
    columns: hasMetadata ? content.columns : [],
  });
}

/** État de la barre de filtres que donne un préréglage. */
export function applyPreset(content: FilterPresetContent): FilterBarState {
  return { tabs: [...content.tabs], filter: content.filter, enabled: true };
}
