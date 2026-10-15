import { commands } from "./bindings";
import type {
  ImportPresetDto,
  ImportPresetInput,
  ImportPresetSettingsDto,
} from "./bindings";

export type { ImportPresetDto, ImportPresetInput, ImportPresetSettingsDto };

/**
 * Préréglages d'import enregistrés, par ordre alphabétique.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchImportPresets(): Promise<ImportPresetDto[]> {
  const result = await commands.getImportPresets();
  if (result.status === "error") {
    throw new Error(`Import presets unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Enregistre les réglages sous `name` (un préréglage du même nom est
 * remplacé).
 *
 * @throws si le cœur ne répond pas ou refuse.
 */
export async function saveImportPreset(
  name: string,
  settings: ImportPresetInput,
): Promise<void> {
  const result = await commands.saveImportPreset(name, settings);
  if (result.status === "error") {
    throw new Error(`Cannot save the import preset: ${result.error.kind}`);
  }
}

/**
 * Reprend un préréglage : ses dossiers deviennent les dossiers choisis.
 *
 * @throws si le cœur ne répond pas, ou si le préréglage est inconnu ou
 *   illisible.
 */
export async function applyImportPreset(
  id: number,
): Promise<ImportPresetSettingsDto> {
  const result = await commands.applyImportPreset(id);
  if (result.status === "error") {
    throw new Error(`Cannot apply the import preset: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Supprime un préréglage.
 *
 * @throws si le cœur ne répond pas.
 */
export async function deleteImportPreset(id: number): Promise<void> {
  const result = await commands.deleteImportPreset(id);
  if (result.status === "error") {
    throw new Error(`Cannot delete the import preset: ${result.error.kind}`);
  }
}
