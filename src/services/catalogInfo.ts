import { commands } from "./bindings";
import type { CatalogOverview } from "./bindings";

export type { CatalogOverview };

/**
 * Nombre de photos et de vidéos du catalogue ouvert.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchCatalogOverview(): Promise<CatalogOverview> {
  const result = await commands.getCatalogOverview();
  if (result.status === "error") {
    throw new Error(`Catalog overview unavailable: ${result.error.kind}`);
  }
  return result.data;
}
