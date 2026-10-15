import { commands } from "./bindings";
import type {
  ImportChoicesDto,
  ImportPlacementDto,
  ImportStartFailure,
  ImportStarted,
  ImportSummary,
} from "./bindings";

export type {
  ImportPlacementDto,
  ImportStartFailure,
  ImportStarted,
  ImportSummary,
};

/**
 * Choix faits dans la fenêtre d'import : doublons, modèle de métadonnées
 * (`null` : aucun), et fichiers laissés à leur place ou copiés.
 */
export type ImportChoices = ImportChoicesDto;

/**
 * Lance l'import du dossier d'une analyse terminée, par référence ou par
 * copie ; la progression et le bilan arrivent par l'abonnement aux tâches.
 * Les choix sont retenus pour l'import suivant.
 *
 * @param analysisJobId identifiant de la tâche d'analyse du dossier.
 * @throws si le cœur ne répond pas.
 */
export function importAnalyzedFolder(
  analysisJobId: number,
  choices: ImportChoices,
): Promise<
  | { status: "ok"; data: ImportStarted }
  | { status: "error"; error: ImportStartFailure }
> {
  return commands.importAnalyzedFolder(analysisJobId, choices);
}
