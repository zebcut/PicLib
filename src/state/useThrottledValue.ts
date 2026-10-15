import { useEffect, useRef, useState } from "react";

/**
 * Suit `value`, mais au plus une fois par intervalle : une valeur qui change
 * dix fois par seconde n'en provoque qu'un changement par intervalle, la
 * dernière valeur étant toujours transmise à la fin.
 */
export function useThrottledValue<T>(value: T, intervalMs: number): T {
  const [throttled, setThrottled] = useState(value);
  const lastEmission = useRef(0);

  useEffect(() => {
    const wait = Math.max(0, lastEmission.current + intervalMs - Date.now());
    const timer = setTimeout(() => {
      lastEmission.current = Date.now();
      setThrottled(value);
    }, wait);
    return () => {
      clearTimeout(timer);
    };
  }, [value, intervalMs]);

  return throttled;
}
