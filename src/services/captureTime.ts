import { commands } from "./bindings";
import type { CaptureTimeChangeDto, CaptureTimeState } from "./bindings";
import { toEditResult } from "./edits";
import type { EditResult } from "./edits";

export type { CaptureTimeChangeDto, CaptureTimeState };

/**
 * Donne une date de prise de vue aux médias indiqués, ou leur rend leur date
 * d'origine, de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function changeCaptureTime(
  mediaIds: number[],
  change: CaptureTimeChangeDto,
): Promise<EditResult> {
  return toEditResult(await commands.changeCaptureTime(mediaIds, change));
}
