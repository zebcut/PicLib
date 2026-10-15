import { useEffect, useState } from "react";

import { fetchImportPreferences } from "../services/importDestination";
import type { ImportPreferencesDto } from "../services/importDestination";

/** Choix du dernier import, lus à l'ouverture de la fenêtre d'import. */
export type ImportPreferencesState =
  | { status: "loading" }
  | { status: "ready"; preferences: ImportPreferencesDto }
  /** Illisibles : l'import reste possible, par référence seulement. */
  | { status: "unavailable" };

/**
 * Les choix du dernier import, à présélectionner.
 *
 * @param isWanted faux : rien n'est lu (import impossible).
 */
export function useImportPreferences(
  isWanted: boolean,
): ImportPreferencesState {
  const [state, setState] = useState<ImportPreferencesState>({
    status: "loading",
  });

  useEffect(() => {
    if (!isWanted) return;
    let isActive = true;
    fetchImportPreferences().then(
      (preferences) => {
        if (isActive) setState({ status: "ready", preferences });
      },
      (error: unknown) => {
        console.error("Failed to read the import choices", error);
        if (isActive) setState({ status: "unavailable" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [isWanted]);

  return state;
}
