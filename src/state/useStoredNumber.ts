import { useCallback, useState } from "react";

/**
 * Nombre retenu d'une session à l'autre dans le navigateur intégré (une
 * préférence d'affichage, jamais une donnée du catalogue), borné entre `min`
 * et `max`. Si le stockage est indisponible, la valeur par défaut est
 * utilisée et le changement vaut pour la session.
 */
export function useStoredNumber(
  key: string,
  defaultValue: number,
  min: number,
  max: number,
): [number, (value: number) => void] {
  const [value, setValue] = useState(() =>
    clamp(readStored(key) ?? defaultValue, min, max),
  );

  const update = useCallback(
    (next: number) => {
      const bounded = clamp(next, min, max);
      setValue(bounded);
      try {
        localStorage.setItem(key, String(bounded));
      } catch (error: unknown) {
        console.warn(`Cannot remember ${key}`, error);
      }
    },
    [key, min, max],
  );

  return [value, update];
}

function readStored(key: string): number | undefined {
  try {
    const stored = localStorage.getItem(key);
    if (stored === null) return undefined;
    const parsed = Number(stored);
    return Number.isFinite(parsed) ? parsed : undefined;
  } catch {
    // Stockage refusé : la valeur par défaut convient.
    return undefined;
  }
}

function clamp(value: number, min: number, max: number): number {
  return Math.min(max, Math.max(min, value));
}
