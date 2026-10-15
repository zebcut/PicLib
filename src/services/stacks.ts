import { commands } from "./bindings";
import type {
  AutoStackMethod,
  AutoStackPreview,
  AutoStackScope,
  GridStack,
  StackFailure,
  StackOperation,
} from "./bindings";
import type { EditResult } from "./edits";

export type {
  AutoStackMethod,
  AutoStackPreview,
  AutoStackScope,
  GridStack,
  StackOperation,
};

/** Refus d'un changement (moins de deux photos, plusieurs dossiers…). */
export type StackRefusal = Exclude<StackFailure["kind"], "catalogUnavailable">;

/** Résultat d'un changement : fait (annulable) ou refusé. */
export type StackChangeResult =
  | { status: "ok"; result: EditResult }
  | { status: "refused"; refusal: StackRefusal };

/**
 * Empile ou dépile des photos, de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function changeStacks(
  operation: StackOperation,
): Promise<StackChangeResult> {
  const result = await commands.changeStacks(operation);
  if (result.status === "ok") {
    return { status: "ok", result: { status: "ok", outcome: result.data } };
  }
  return result.error.kind === "catalogUnavailable"
    ? {
        status: "ok",
        result: { status: "error", failure: { kind: "catalogUnavailable" } },
      }
    : { status: "refused", refusal: result.error.kind };
}

/**
 * Déplie les piles des photos indiquées si l'une est repliée, les replie
 * sinon ; renvoie le nouvel état (`null` : aucune pile). Hors de Cmd+Z.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function toggleStacks(
  mediaIds: number[],
): Promise<boolean | null> {
  const result = await commands.toggleStacks(mediaIds);
  if (result.status === "error") {
    throw new Error(`Stacks not toggled: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Ce qu'un empilement automatique ferait (nombre de piles et de photos),
 * pour le confirmer.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchAutoStacking(
  scope: AutoStackScope,
  method: AutoStackMethod,
): Promise<AutoStackPreview> {
  const result = await commands.getAutoStacking(scope, method);
  if (result.status === "error") {
    throw new Error(`Auto stacking unavailable: ${result.error.kind}`);
  }
  return result.data;
}
