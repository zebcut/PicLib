import { useEffect, useState } from "react";

import { fetchFilterPresets } from "../services/filterPresets";
import type { FilterPresetDto } from "../services/filterPresets";

/**
 * Préréglages de filtres de l'utilisateur, relus quand `refreshKey` change
 * (chaque modification du catalogue, dont la création ou l'annulation d'un
 * préréglage). Une liste vide tant qu'ils ne sont pas lus, ou si la lecture
 * échoue (journalisée) : les préréglages fournis restent proposés.
 */
export function useFilterPresets(refreshKey: number): FilterPresetDto[] {
  const [presets, setPresets] = useState<FilterPresetDto[]>([]);

  useEffect(() => {
    let isActive = true;
    fetchFilterPresets().then(
      (loaded) => {
        if (isActive) setPresets(loaded);
      },
      (error: unknown) => {
        console.error("Failed to read the filter presets", error);
      },
    );
    return () => {
      isActive = false;
    };
  }, [refreshKey]);

  return presets;
}
