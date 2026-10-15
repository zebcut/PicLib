import { useCallback, useState } from "react";

import type { GridMetadataColumn } from "../services/metadataBrowser";
import {
  DEFAULT_METADATA_COLUMNS,
  MAX_METADATA_COLUMNS,
  METADATA_COLUMNS,
} from "./gridFilter";

/** Clé des colonnes du filtre Métadonnées, retenues d'une session à l'autre. */
const STORAGE_KEY = "piclib.grid.metadataColumns";

/**
 * Colonnes du filtre Métadonnées (leurs critères, dans l'ordre), retenues
 * d'une session à l'autre comme `useStoredNumber` : une valeur enregistrée
 * illisible ou inconnue redonne les colonnes par défaut.
 */
export function useStoredColumns(): [
  readonly GridMetadataColumn[],
  (columns: readonly GridMetadataColumn[]) => void,
] {
  const [columns, setColumns] = useState(readStored);

  const update = useCallback((next: readonly GridMetadataColumn[]) => {
    setColumns(next);
    try {
      localStorage.setItem(STORAGE_KEY, JSON.stringify(next));
    } catch (error: unknown) {
      console.warn(`Cannot remember ${STORAGE_KEY}`, error);
    }
  }, []);

  return [columns, update];
}

function readStored(): readonly GridMetadataColumn[] {
  try {
    const stored = localStorage.getItem(STORAGE_KEY);
    if (stored === null) return DEFAULT_METADATA_COLUMNS;
    const parsed: unknown = JSON.parse(stored);
    const isValid =
      Array.isArray(parsed) &&
      parsed.length > 0 &&
      parsed.length <= MAX_METADATA_COLUMNS &&
      parsed.every((column) =>
        METADATA_COLUMNS.includes(column as GridMetadataColumn),
      );
    return isValid
      ? (parsed as GridMetadataColumn[])
      : DEFAULT_METADATA_COLUMNS;
  } catch {
    // Stockage refusé ou texte illisible : les colonnes par défaut.
    return DEFAULT_METADATA_COLUMNS;
  }
}
