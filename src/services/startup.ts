import { commands } from "./bindings";
import type { StartupFailure, StartupStatus } from "./bindings";

export type { StartupFailure, StartupStatus };

/**
 * Demande au cœur Rust si le catalogue a pu être ouvert au démarrage.
 *
 * @throws si le cœur ne répond pas (erreur de communication avec Tauri).
 */
export function fetchStartupStatus(): Promise<StartupStatus> {
  return commands.getStartupStatus();
}
