import { commands } from "./bindings";
import type {
  BackupFrequencyDto,
  CatalogBackupView,
  PreferencesFailure,
  PreferencesInput,
  PreferencesView,
  PreviewCacheView,
} from "./bindings";

export type {
  BackupFrequencyDto,
  CatalogBackupView,
  PreferencesFailure,
  PreferencesInput,
  PreferencesView,
  PreviewCacheView,
};

/**
 * Préférences en vigueur, place occupée par les aperçus et date de la
 * dernière sauvegarde automatique.
 *
 * @throws si le cœur ne répond pas.
 */
export function fetchPreferences(): Promise<PreferencesView> {
  return commands.getPreferences();
}

/**
 * Enregistre et applique de nouvelles préférences ; renvoie la raison d'un
 * refus, ou `null` si tout s'est bien passé.
 *
 * @throws si le cœur ne répond pas.
 */
export async function savePreferences(
  input: PreferencesInput,
): Promise<PreferencesFailure | null> {
  const result = await commands.setPreferences(input);
  return result.status === "error" ? result.error : null;
}
