import { Channel } from "@tauri-apps/api/core";

import { commands } from "./bindings";
import type { EjectFailure, ImportRequestDto } from "./bindings";

export type { EjectFailure, ImportRequestDto };

/**
 * S'abonne aux demandes d'ouverture de l'écran d'import : une carte
 * branchée (si la préférence est cochée), un dossier glissé du Finder.
 *
 * @throws si le cœur ne répond pas.
 */
export async function subscribeToImportRequests(
  onRequest: (request: ImportRequestDto) => void,
): Promise<void> {
  const channel = new Channel<ImportRequestDto>();
  channel.onmessage = onRequest;
  await commands.subscribeImportRequests(channel);
}

/**
 * Éjecte la carte mémoire d'un import terminé.
 *
 * @throws si le cœur ne répond pas.
 */
export function ejectCard(
  importJobId: number,
): Promise<
  { status: "ok"; data: null } | { status: "error"; error: EjectFailure }
> {
  return commands.ejectCard(importJobId);
}
