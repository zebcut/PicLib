/**
 * Remaining time of a background job, estimated from its progress since the
 * first update that gave a total (the average speed so far).
 */

/** First progress seen against the current total. */
export interface ProgressStart {
  /** When it was received, in milliseconds (`Date.now()`). */
  time: number;
  progress: number;
  total: number;
}

/**
 * No estimate before this much time has passed: the first files are often
 * slower or faster than the rest (disk cache, small files first).
 */
export const MIN_ESTIMATE_DELAY_MS = 3_000;

/**
 * Starting point to estimate from, after an update: kept while the total
 * stays the same and the progress moves forward, reset otherwise (the job
 * entered another phase, like an analysis going from finding files to
 * reading their dates).
 */
export function nextStart(
  start: ProgressStart | undefined,
  progress: number,
  total: number,
  time: number,
): ProgressStart {
  if (start?.total !== total || progress < start.progress) {
    return { time, progress, total };
  }
  return start;
}

/**
 * Milliseconds left at the average speed since `start`; `null` while there
 * is not enough to go on.
 */
export function remainingMs(
  start: ProgressStart,
  progress: number,
  total: number,
  time: number,
): number | null {
  const elapsed = time - start.time;
  const done = progress - start.progress;
  if (elapsed < MIN_ESTIMATE_DELAY_MS || done <= 0) return null;
  return (elapsed / done) * Math.max(0, total - progress);
}
