import { useCallback, useState } from "react";

/**
 * Au-delà, les identifiants les plus anciens sont oubliés. Une liste de
 * mots-clés peut compter des dizaines de milliers de branches (Option+clic
 * les déplie toutes) ; 30 000 identifiants tiennent en 200 Ko.
 */
const MAX_STORED_IDS = 30_000;

/**
 * Ensemble d'identifiants retenu d'une session à l'autre dans le
 * navigateur intégré (les branches dépliées d'un arbre : une préférence
 * d'affichage, jamais une donnée du catalogue). Si le stockage est
 * indisponible, l'ensemble vaut pour la session.
 *
 * @returns l'ensemble, et de quoi y mettre ou en retirer un ou plusieurs
 *   identifiants.
 */
export function useStoredIdSet(
  key: string,
): [
  ReadonlySet<number>,
  (ids: number | readonly number[], present: boolean) => void,
] {
  const [ids, setIds] = useState<ReadonlySet<number>>(() => readStored(key));

  const update = useCallback(
    (changed: number | readonly number[], present: boolean) => {
      const list = typeof changed === "number" ? [changed] : changed;
      setIds((previous) => {
        if (list.every((id) => previous.has(id) === present)) return previous;
        const next = new Set(previous);
        for (const id of list) {
          if (present) next.add(id);
          else next.delete(id);
        }
        // Un ensemble garde l'ordre d'ajout : les plus anciens d'abord.
        const kept = [...next].slice(-MAX_STORED_IDS);
        try {
          localStorage.setItem(key, JSON.stringify(kept));
        } catch (error: unknown) {
          console.warn(`Cannot remember ${key}`, error);
        }
        return new Set(kept);
      });
    },
    [key],
  );

  return [ids, update];
}

function readStored(key: string): ReadonlySet<number> {
  try {
    const stored = localStorage.getItem(key);
    if (stored === null) return new Set();
    const parsed: unknown = JSON.parse(stored);
    if (!Array.isArray(parsed)) return new Set();
    return new Set(parsed.filter((id) => Number.isInteger(id)) as number[]);
  } catch {
    // Stockage refusé ou valeur illisible : on repart de rien.
    return new Set();
  }
}
