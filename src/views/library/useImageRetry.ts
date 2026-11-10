import { useCallback, useEffect, useRef, useState } from "react";

/**
 * Delays before asking again for an image that failed to load: a thumbnail
 * that exists can still fail to be read for a moment (a sync client holding
 * the file just after it was written).
 */
export const IMAGE_RETRY_DELAYS_MS = [2_000, 5_000, 15_000] as const;

/** Where a failing image stands. */
export interface ImageRetry {
  /** Number of new requests made so far (0: the first one). */
  attempt: number;
  /** True once every new request has failed too: show a placeholder. */
  gaveUp: boolean;
  /** To call from the image's `onError`. */
  onError: () => void;
}

/**
 * Asks again for an image that failed to load, a few times, then gives up.
 * Everything starts over when `key` changes (another image, or a new
 * version of it).
 */
export function useImageRetry(key: string): ImageRetry {
  const [state, setState] = useState({ key, attempt: 0, gaveUp: false });
  if (state.key !== key) {
    setState({ key, attempt: 0, gaveUp: false });
  }
  const timer = useRef<number | null>(null);
  const clearTimer = () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
  };
  // A pending new request belongs to the previous image: dropped.
  useEffect(() => clearTimer, [key]);

  const { attempt } = state.key === key ? state : { attempt: 0 };
  const onError = useCallback(() => {
    clearTimer();
    const delay = IMAGE_RETRY_DELAYS_MS[attempt];
    if (delay === undefined) {
      setState((current) =>
        current.key === key ? { ...current, gaveUp: true } : current,
      );
      return;
    }
    timer.current = window.setTimeout(() => {
      timer.current = null;
      setState((current) =>
        current.key === key
          ? { ...current, attempt: current.attempt + 1 }
          : current,
      );
    }, delay);
  }, [attempt, key]);

  return {
    attempt,
    gaveUp: state.key === key && state.gaveUp,
    onError,
  };
}

/**
 * Image version for a new request: another number makes the web view ask
 * the core again instead of reusing the failed answer. The core ignores it.
 */
export function retryVersion(version: number, attempt: number): number {
  return attempt === 0 ? version : version * 10 + attempt;
}
