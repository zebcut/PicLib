import { useCallback, useMemo, useState } from "react";

import { importAnalyzedFolder } from "../services/importFolder";
import type {
  ImportChoices,
  ImportStartFailure,
} from "../services/importFolder";

/** Échec au lancement d'un import, y compris un cœur qui ne répond pas. */
export type ImportLaunchFailure =
  ImportStartFailure | { kind: "coreUnreachable" };

/** Lancement de l'import des fichiers cochés d'une source analysée. */
export interface FolderImport {
  /**
   * Lance l'import des fichiers cochés de l'analyse `analysisJobId` ;
   * renvoie vrai s'il a démarré.
   */
  start: (analysisJobId: number, choices: ImportChoices) => Promise<boolean>;
  /** Échec du dernier lancement, s'il y en a un à montrer. */
  startFailure: ImportLaunchFailure | null;
  /** Ferme le message d'échec. */
  dismissStartFailure: () => void;
}

/**
 * Lance l'import d'une source analysée. Sa progression et son bilan
 * arrivent ensuite par l'abonnement aux tâches de fond (`useJobs`).
 */
export function useImport(): FolderImport {
  const [startFailure, setStartFailure] = useState<ImportLaunchFailure | null>(
    null,
  );

  const start = useCallback(
    (analysisJobId: number, choices: ImportChoices) =>
      importAnalyzedFolder(analysisJobId, choices).then(
        (result) => {
          if (result.status === "error") {
            setStartFailure(result.error);
            return false;
          }
          return true;
        },
        (error: unknown) => {
          console.error("Failed to start the import", error);
          setStartFailure({ kind: "coreUnreachable" });
          return false;
        },
      ),
    [],
  );

  const dismissStartFailure = useCallback(() => {
    setStartFailure(null);
  }, []);

  return useMemo(
    () => ({ start, startFailure, dismissStartFailure }),
    [start, startFailure, dismissStartFailure],
  );
}
