import { commands } from "./bindings";
import type {
  GridColumnChoice,
  GridColumnValue,
  GridFilter,
  GridMetadataColumn,
  GridSource,
} from "./bindings";

export type { GridColumnChoice, GridColumnValue, GridMetadataColumn };

/**
 * Valeurs d'une colonne du filtre Métadonnées et leur nombre de médias,
 * parmi ceux de la source filtrés par `filter` (dont les choix des colonnes
 * à sa gauche).
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchMetadataColumn(
  source: GridSource,
  filter: GridFilter,
  column: GridMetadataColumn,
): Promise<GridColumnValue[]> {
  const result = await commands.getMetadataColumn(source, filter, column);
  if (result.status === "error") {
    throw new Error(`Metadata column unavailable: ${result.error.kind}`);
  }
  return result.data;
}
