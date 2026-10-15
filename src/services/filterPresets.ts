import { commands } from "./bindings";
import type {
  FilterPresetContent,
  FilterPresetDto,
  FilterPresetFailure,
  GridFilterTab,
} from "./bindings";
import type { EditResult } from "./edits";
import type { GridSource } from "./library";

export type { FilterPresetContent, FilterPresetDto, GridFilterTab };

/**
 * Filtre mémorisé d'une source : onglets et critères (les colonnes du
 * filtre Métadonnées sont communes à toutes les sources).
 */
export type SourceFilterContent = Pick<FilterPresetContent, "tabs" | "filter">;

/** Changement demandé sur un préréglage de l'utilisateur. */
export type FilterPresetOperation =
  | { kind: "create"; name: string; content: FilterPresetContent }
  | { kind: "update"; presetId: number; content: FilterPresetContent }
  | { kind: "rename"; presetId: number; name: string }
  | { kind: "delete"; presetId: number };

/** Refus d'un changement : nom vide, trop long, déjà pris, préréglage disparu. */
export type FilterPresetRefusal = Exclude<
  FilterPresetFailure["kind"],
  "catalogUnavailable"
>;

/** Résultat d'un changement : fait (annulable) ou refusé. */
export type FilterPresetChange =
  | { status: "ok"; result: EditResult }
  | { status: "refused"; refusal: FilterPresetRefusal };

/**
 * Préréglages de l'utilisateur, par ordre alphabétique.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchFilterPresets(): Promise<FilterPresetDto[]> {
  const result = await commands.getFilterPresets();
  if (result.status === "error") {
    throw new Error(`Filter presets unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Crée, met à jour, renomme ou supprime un préréglage, de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function changeFilterPreset(
  operation: FilterPresetOperation,
): Promise<FilterPresetChange> {
  const result = await (() => {
    switch (operation.kind) {
      case "create":
        return commands
          .createFilterPreset(operation.name, operation.content)
          .then((created) =>
            created.status === "ok"
              ? { status: "ok" as const, data: created.data.outcome }
              : created,
          );
      case "update":
        return commands.updateFilterPreset(
          operation.presetId,
          operation.content,
        );
      case "rename":
        return commands.renameFilterPreset(operation.presetId, operation.name);
      case "delete":
        return commands.deleteFilterPreset(operation.presetId);
    }
  })();
  if (result.status === "ok") {
    return { status: "ok", result: { status: "ok", outcome: result.data } };
  }
  const failure = result.error;
  return failure.kind === "catalogUnavailable"
    ? {
        status: "ok",
        result: {
          status: "error",
          failure: { kind: "catalogUnavailable" },
        },
      }
    : { status: "refused", refusal: failure.kind };
}

/**
 * Vrai si l'option « Mémoriser un filtre pour chaque dossier ou mot-clé »
 * est cochée dans les Préférences.
 *
 * @throws si le cœur ne répond pas.
 */
export async function fetchRemembersFilterPerSource(): Promise<boolean> {
  const preferences = await commands.getPreferences();
  return preferences.rememberFilterPerSource;
}

/**
 * Filtre mémorisé d'une source, ou `null`.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchSourceFilter(
  source: GridSource,
): Promise<SourceFilterContent | null> {
  const result = await commands.getSourceFilter(source);
  if (result.status === "error") {
    throw new Error(`Source filter unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Mémorise le filtre d'une source (`null` : l'oublie).
 *
 * @throws si le cœur ne répond pas ou si le catalogue ne peut pas être
 *   écrit.
 */
export async function saveSourceFilter(
  source: GridSource,
  content: SourceFilterContent | null,
): Promise<void> {
  const result = await commands.saveSourceFilter(source, content);
  if (result.status === "error") {
    throw new Error(`Source filter not saved: ${result.error.kind}`);
  }
}
