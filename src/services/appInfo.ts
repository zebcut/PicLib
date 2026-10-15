import { commands } from "./bindings";
import type { AppInfo } from "./bindings";

export type { AppInfo };

/**
 * Demande au cœur Rust le nom et la version de l'application.
 *
 * @throws si le cœur ne répond pas (erreur de communication avec Tauri).
 */
export function fetchAppInfo(): Promise<AppInfo> {
  return commands.getAppInfo();
}
