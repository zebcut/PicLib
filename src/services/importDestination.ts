import { commands } from "./bindings";
import type {
  BackupChoice,
  DestinationChoice,
  ImportModeDto,
  ImportPreferencesDto,
} from "./bindings";

export type {
  BackupChoice,
  DestinationChoice,
  ImportModeDto,
  ImportPreferencesDto,
};

/**
 * Choix du dernier import (Laisser ou Copier, destination, chemin d'origine
 * dans la légende), à présélectionner dans la fenêtre d'import.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchImportPreferences(): Promise<ImportPreferencesDto> {
  const result = await commands.getImportPreferences();
  if (result.status === "error") {
    throw new Error(`Cannot read the import choices: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Fait choisir le dossier où copier les fichiers. Le cœur garde le dossier
 * choisi : il est ensuite désigné par `"chosen"`, jamais renvoyé.
 *
 * @returns le dossier choisi, à afficher, ou `null` si la fenêtre a été
 *   fermée sans choix.
 * @throws si le cœur ne répond pas ou si l'emplacement est inutilisable.
 */
export async function chooseImportDestination(
  dialogTitle: string,
): Promise<string | null> {
  const result = await commands.chooseImportDestination(dialogTitle);
  if (result.status === "error") {
    throw new Error(`Cannot choose the destination: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Fait choisir le dossier de la copie de sauvegarde. Le cœur le garde : il
 * est ensuite désigné par `"chosen"`, jamais renvoyé.
 *
 * @returns le dossier choisi, à afficher, ou `null` si la fenêtre a été
 *   fermée sans choix.
 * @throws si le cœur ne répond pas ou si l'emplacement est inutilisable.
 */
export async function chooseBackupDestination(
  dialogTitle: string,
): Promise<string | null> {
  const result = await commands.chooseBackupDestination(dialogTitle);
  if (result.status === "error") {
    throw new Error(`Cannot choose the backup folder: ${result.error.kind}`);
  }
  return result.data;
}
