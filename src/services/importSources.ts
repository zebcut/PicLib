import { commands } from "./bindings";
import type {
  AnalyzeFolderFailure,
  ArrangementDto,
  DatePart,
  DestinationLayout,
  DestinationPreviewDto,
  DestinationPreviewRequest,
  ImportFileDto,
  ImportFileStatus,
  ImportSourceDto,
  ImportSourceKind,
  ImportSourcesDto,
  PlannedFolderDto,
} from "./bindings";

export type {
  AnalyzeFolderFailure,
  ArrangementDto,
  DatePart,
  DestinationLayout,
  DestinationPreviewDto,
  DestinationPreviewRequest,
  ImportFileDto,
  ImportFileStatus,
  ImportSourceDto,
  ImportSourceKind,
  ImportSourcesDto,
  PlannedFolderDto,
};

/**
 * Sources de l'écran d'import : cartes mémoire branchées et derniers
 * dossiers choisis.
 *
 * @throws si le cœur ne répond pas.
 */
export function listImportSources(): Promise<ImportSourcesDto> {
  return commands.listImportSources();
}

/**
 * Retire un dossier des derniers dossiers choisis (le dossier lui-même
 * n'est pas touché).
 *
 * @throws si le cœur ne répond pas.
 */
export async function forgetImportSource(sourceId: number): Promise<void> {
  await commands.forgetImportSource(sourceId);
}

/**
 * Fait choisir un dossier à importer.
 *
 * @returns la source choisie, ou `null` si la fenêtre a été fermée.
 * @throws si le cœur ne répond pas.
 */
export function chooseImportSource(
  dialogTitle: string,
): Promise<
  | { status: "ok"; data: ImportSourceDto | null }
  | { status: "error"; error: AnalyzeFolderFailure }
> {
  return commands.chooseImportSource(dialogTitle);
}

/**
 * Lance l'analyse d'une source en tâche de fond ; sa progression arrive par
 * l'abonnement aux tâches.
 *
 * @returns le numéro de la tâche d'analyse.
 * @throws si le cœur ne répond pas.
 */
export function analyzeImportSource(
  sourceId: number,
  includeSubfolders: boolean,
): Promise<
  | { status: "ok"; data: number }
  | { status: "error"; error: AnalyzeFolderFailure }
> {
  return commands.analyzeImportSource(sourceId, includeSubfolders);
}

/**
 * Fichiers d'une analyse terminée, dans l'ordre des prises de vue.
 *
 * @throws si le cœur ne répond pas ou si l'analyse est inconnue.
 */
export async function fetchAnalyzedFiles(
  analysisJobId: number,
): Promise<ImportFileDto[]> {
  const result = await commands.getAnalyzedFiles(analysisJobId);
  if (result.status === "error") {
    throw new Error(`Analyzed files unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Aperçu de l'arborescence (et des premiers noms, pour un renommage) que
 * créerait la copie des fichiers cochés ; rien n'est écrit.
 *
 * @throws si le cœur ne répond pas ou si l'analyse est inconnue.
 */
export async function fetchDestinationPreview(
  analysisJobId: number,
  request: DestinationPreviewRequest,
): Promise<DestinationPreviewDto> {
  const result = await commands.getDestinationPreview(analysisJobId, request);
  if (result.status === "error") {
    throw new Error(`Destination preview unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Oublie une analyse (écran d'import fermé, autre source) et efface ses
 * vignettes.
 *
 * @throws si le cœur ne répond pas.
 */
export async function forgetAnalysis(analysisJobId: number): Promise<void> {
  await commands.forgetAnalysis(analysisJobId);
}
