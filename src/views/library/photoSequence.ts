/**
 * Photos parcourues par le mode Tri plein écran et la vue
 * Comparaison : toutes celles de l'ensemble affiché, ou
 * seulement les photos sélectionnées quand il y en a plusieurs (comme
 * Lightroom). Fonctions pures, sur les positions des photos dans l'ensemble
 * affiché (celles de la grille).
 */

/** Photos parcourues, dans l'ordre de la grille. */
export interface PhotoSequence {
  /** Nombre de photos parcourues. */
  readonly length: number;
  /** Position, dans l'ensemble affiché, de la photo de rang `step`. */
  positionAt: (step: number) => number | undefined;
  /** Rang de la photo située à cette position ; `undefined` si elle n'est pas parcourue. */
  stepOf: (position: number) => number | undefined;
}

/** Photos chargées à l'avance après la photo affichée : le tri avance. */
export const PRELOAD_AHEAD = 3;

/** Photos chargées à l'avance avant la photo affichée. */
export const PRELOAD_BEHIND = 1;

/** Toutes les photos de l'ensemble affiché, qui en compte `count`. */
export function wholeSource(count: number): PhotoSequence {
  const inRange = (value: number) =>
    Number.isInteger(value) && value >= 0 && value < count ? value : undefined;
  return { length: count, positionAt: inRange, stepOf: inRange };
}

/**
 * Les photos sélectionnées seulement, dans l'ordre de la grille.
 *
 * @param allIds identifiants de toutes les photos de l'ensemble affiché,
 *   dans l'ordre de la grille.
 */
export function selectedOnly(
  allIds: readonly number[],
  selected: ReadonlySet<number>,
): PhotoSequence {
  // Croissantes, puisque relevées dans l'ordre de la grille.
  const positions: number[] = [];
  allIds.forEach((id, position) => {
    if (selected.has(id)) positions.push(position);
  });
  return {
    length: positions.length,
    positionAt: (step) => positions[step],
    stepOf: (position) => binarySearch(positions, position),
  };
}

/**
 * Position de la photo située `delta` rangs après celle de la position
 * `position` (avant si `delta` est négatif) ; `undefined` au-delà des
 * extrémités, ou si cette photo n'est pas parcourue.
 */
export function positionFrom(
  sequence: PhotoSequence,
  position: number,
  delta: number,
): number | undefined {
  const step = sequence.stepOf(position);
  return step === undefined ? undefined : sequence.positionAt(step + delta);
}

/**
 * Positions des photos à charger à l'avance autour de celle de la position
 * `position` : d'abord les suivantes, puis la précédente.
 */
export function preloadPositions(
  sequence: PhotoSequence,
  position: number,
): number[] {
  const deltas = [
    ...Array.from({ length: PRELOAD_AHEAD }, (_, index) => index + 1),
    ...Array.from({ length: PRELOAD_BEHIND }, (_, index) => -(index + 1)),
  ];
  return deltas
    .map((delta) => positionFrom(sequence, position, delta))
    .filter((neighbor) => neighbor !== undefined);
}

/** Rang de `value` dans la liste croissante `sorted`, s'il s'y trouve. */
function binarySearch(
  sorted: readonly number[],
  value: number,
): number | undefined {
  let low = 0;
  let high = sorted.length - 1;
  while (low <= high) {
    const middle = Math.floor((low + high) / 2);
    const current = sorted[middle];
    if (current === undefined) return undefined;
    if (current === value) return middle;
    if (current < value) low = middle + 1;
    else high = middle - 1;
  }
  return undefined;
}
