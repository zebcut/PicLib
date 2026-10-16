import { commands } from "./bindings";
import type { CatalogCheckSummary, CatalogCleanupSummary } from "./bindings";

export type { CatalogCheckSummary, CatalogCleanupSummary };

/**
 * Starts the check of the catalog (File ▸ Clean Up and Optimize Catalog…):
 * its summary arrives through the background jobs subscription. Nothing is
 * changed.
 *
 * @throws si le cœur ne répond pas.
 */
export function checkCatalog(): Promise<void> {
  return commands.checkCatalog();
}

/**
 * Starts the cleanup of the catalog, after the user confirmed the summary
 * of a check: verified backup, removal of the orphan thumbnails and
 * previews, then rebuild of the database.
 *
 * @throws si le cœur ne répond pas.
 */
export function cleanCatalog(): Promise<void> {
  return commands.cleanCatalog();
}

/** True when a cleanup would remove or reclaim something. */
export function hasSomethingToClean(summary: CatalogCheckSummary): boolean {
  return (
    summary.orphanThumbnails + summary.orphanPreviews + summary.loneStacks >
      0 || summary.reclaimableBytes > 0
  );
}
