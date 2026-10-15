import { useCallback } from "react";

import { useStoredNumber } from "./useStoredNumber";

/**
 * Oui/non retenu d'une session à l'autre dans le navigateur intégré (une
 * préférence d'affichage), avec les mêmes garanties que `useStoredNumber` :
 * enregistré comme 0 ou 1.
 */
export function useStoredBoolean(
  key: string,
  defaultValue: boolean,
): [boolean, (value: boolean) => void] {
  const [stored, setStored] = useStoredNumber(key, defaultValue ? 1 : 0, 0, 1);
  const update = useCallback(
    (value: boolean) => {
      setStored(value ? 1 : 0);
    },
    [setStored],
  );
  return [stored === 1, update];
}
