/**
 * Candidates de la vue Comparaison : la « sélection » est la
 * photo de référence, la « candidate » celle qu'on lui compare, prise parmi
 * les photos parcourues (voir `photoSequence.ts`). Fonctions pures, sur les
 * positions des photos dans l'ensemble affiché.
 */
import { positionFrom } from "./photoSequence";
import type { PhotoSequence } from "./photoSequence";

/**
 * Position de la candidate voisine de celle de la position `from`, dans le
 * sens `direction`, en sautant la sélection (qui ne peut pas être sa propre
 * candidate) ; `undefined` au bout de la liste.
 */
export function nextCandidate(
  sequence: PhotoSequence,
  selectPosition: number,
  from: number,
  direction: 1 | -1,
): number | undefined {
  const target = positionFrom(sequence, from, direction);
  return target === selectPosition
    ? positionFrom(sequence, selectPosition, direction)
    : target;
}

/**
 * Candidate proposée pour une sélection : la photo qui la suit, ou celle
 * qui la précède si c'est la dernière ; `undefined` s'il n'y a qu'une photo.
 */
export function initialCandidate(
  sequence: PhotoSequence,
  selectPosition: number,
): number | undefined {
  return (
    nextCandidate(sequence, selectPosition, selectPosition, 1) ??
    nextCandidate(sequence, selectPosition, selectPosition, -1)
  );
}
