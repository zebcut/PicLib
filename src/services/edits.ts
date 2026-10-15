import { commands } from "./bindings";
import type {
  ColorLabelDto,
  EditChangeDto,
  EditFailure,
  EditOutcome,
  EditSummaryDto,
  FlagDto,
  HistoryState,
  RotationDirectionDto,
  TriageChange,
} from "./bindings";

export type {
  ColorLabelDto,
  EditChangeDto,
  EditFailure,
  EditOutcome,
  EditSummaryDto,
  FlagDto,
  HistoryState,
  RotationDirectionDto,
  TriageChange,
};

/** Résultat d'un appel au cœur qui modifie le catalogue. */
export type EditResult =
  | { status: "ok"; outcome: EditOutcome }
  | { status: "error"; failure: EditFailure };

/**
 * Donne un marqueur, une note ou une couleur aux médias indiqués, de façon
 * annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function setTriage(
  mediaIds: number[],
  change: TriageChange,
): Promise<EditResult> {
  return toEditResult(await commands.setTriage(mediaIds, change));
}

/**
 * Fait pivoter les médias indiqués d'un quart de tour, de façon annulable
 * (affichage seulement : les fichiers ne sont jamais modifiés).
 *
 * @throws si le cœur ne répond pas.
 */
export async function rotate(
  mediaIds: number[],
  direction: RotationDirectionDto,
): Promise<EditResult> {
  return toEditResult(await commands.rotate(mediaIds, direction));
}

/**
 * Annule la dernière modification.
 *
 * @throws si le cœur ne répond pas.
 */
export async function undoEdit(): Promise<EditResult> {
  return toEditResult(await commands.undoEdit());
}

/**
 * Rétablit la dernière modification annulée.
 *
 * @throws si le cœur ne répond pas.
 */
export async function redoEdit(): Promise<EditResult> {
  return toEditResult(await commands.redoEdit());
}

/**
 * Ce que Cmd+Z et Cmd+Maj+Z feraient maintenant.
 *
 * @throws si le cœur ne répond pas.
 */
export function fetchEditHistory(): Promise<HistoryState> {
  return commands.getEditHistory();
}

/** Résultat d'un appel au cœur qui modifie le catalogue, sous une forme commune. */
export function toEditResult(
  result:
    | { status: "ok"; data: EditOutcome }
    | { status: "error"; error: EditFailure },
): EditResult {
  return result.status === "ok"
    ? { status: "ok", outcome: result.data }
    : { status: "error", failure: result.error };
}
