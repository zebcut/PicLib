import { useEffect, useState } from "react";

import { fetchFolderPanel } from "../services/folders";
import type { FolderPanelDto } from "../services/folders";

/** Contenu du panneau Dossiers, lu dans le cœur. */
export type FolderPanelState =
  | { status: "loading" }
  | { status: "ready"; panel: FolderPanelDto }
  | { status: "failed" };

/**
 * Disques et dossiers du catalogue, relus quand `refreshKey` change
 * (import, modification…). Le contenu précédent reste affiché pendant la
 * lecture.
 */
export function useFolderPanel(refreshKey: number): FolderPanelState {
  const [state, setState] = useState<FolderPanelState>({ status: "loading" });

  useEffect(() => {
    let isActive = true;
    fetchFolderPanel().then(
      (panel) => {
        if (isActive) setState({ status: "ready", panel });
      },
      (error: unknown) => {
        console.error("Failed to read the folder panel", error);
        if (isActive) setState({ status: "failed" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [refreshKey]);

  return state;
}
