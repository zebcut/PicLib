import { useEffect, useState } from "react";

import { fetchMetadataPanel } from "../services/metadata";
import type { MetadataPanel } from "../services/metadata";

/**
 * Délai, en millisecondes, avant de relire le panneau : parcourir les
 * photos aux flèches ne déclenche pas une lecture par photo traversée.
 */
const READ_DELAY_MS = 80;

/** Contenu du panneau Métadonnées, lu dans le cœur. */
export type MetadataPanelState =
  | { status: "empty" }
  | { status: "loading" }
  | { status: "ready"; panel: MetadataPanel }
  | { status: "failed" };

/**
 * Contenu du panneau Métadonnées pour la photo active et la sélection,
 * relu quand elles changent ou quand `refreshKey` change (modification,
 * annulation…). Le contenu précédent reste affiché pendant la lecture.
 */
export function useMetadataPanel(
  activeId: number | null,
  selectedIds: ReadonlySet<number>,
  refreshKey: number,
): MetadataPanelState {
  const [state, setState] = useState<MetadataPanelState>({
    status: "loading",
  });

  useEffect(() => {
    if (activeId === null) return;
    let isActive = true;
    const timer = setTimeout(() => {
      fetchMetadataPanel(activeId, [...selectedIds]).then(
        (panel) => {
          if (isActive) setState({ status: "ready", panel });
        },
        (error: unknown) => {
          console.error("Failed to read the metadata", error);
          if (isActive) setState({ status: "failed" });
        },
      );
    }, READ_DELAY_MS);
    return () => {
      isActive = false;
      clearTimeout(timer);
    };
  }, [activeId, selectedIds, refreshKey]);

  return activeId === null ? { status: "empty" } : state;
}
