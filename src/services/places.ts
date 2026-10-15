import { commands } from "./bindings";
import type { SupportedLanguage } from "../i18n/languages";

/**
 * Annonce au cœur Rust la langue de l'interface. Il y écrit le nom des pays
 * qu'il remplit depuis la position GPS des photos, et lance ce remplissage.
 *
 * @throws si le cœur ne répond pas (erreur de communication avec Tauri).
 */
export function announceInterfaceLanguage(
  language: SupportedLanguage,
): Promise<void> {
  return commands.setInterfaceLanguage(language);
}
