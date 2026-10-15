/**
 * Vue Ensemble : disposition des photos sélectionnées, toutes
 * visibles à la fois, et déplacement de la photo active. Fonctions pures.
 */

/** Écart entre deux photos, et avec les bords, en pixels. */
export const SURVEY_GAP = 8;

/**
 * Proportions supposées des photos pour choisir la disposition : 3:2, les
 * plus courantes. Chaque photo garde ensuite ses vraies proportions dans sa
 * case.
 */
const PHOTO_ASPECT = 3 / 2;

/**
 * Taille, en pixels physiques, au-delà de laquelle la vignette (512 pixels)
 * serait floue : l'aperçu de la Loupe prend alors le relais.
 */
const THUMBNAIL_PIXELS = 512;

/** Disposition des photos : colonnes, rangées, taille des cases et écart. */
export interface SurveyLayout {
  columns: number;
  rows: number;
  cellWidth: number;
  cellHeight: number;
  /** Écart entre les photos et avec les bords, en pixels. */
  gap: number;
}

/**
 * Nombre de colonnes qui donne les plus grandes photos possibles pour
 * `count` photos dans une zone de `width` × `height` pixels. Avec des
 * milliers de photos, les écarts ne laisseraient plus de place : ils
 * disparaissent alors.
 */
export function surveyLayout(
  count: number,
  width: number,
  height: number,
): SurveyLayout {
  return (
    layoutWithGap(count, width, height, SURVEY_GAP) ??
    layoutWithGap(count, width, height, 0) ?? {
      columns: 1,
      rows: Math.max(1, count),
      cellWidth: 0,
      cellHeight: 0,
      gap: 0,
    }
  );
}

/** Meilleure disposition avec cet écart ; `undefined` s'il n'y en a pas. */
function layoutWithGap(
  count: number,
  width: number,
  height: number,
  gap: number,
): SurveyLayout | undefined {
  const innerWidth = width - 2 * gap;
  const innerHeight = height - 2 * gap;
  let best: SurveyLayout | undefined;
  let bestPhotoWidth = 0;
  for (let columns = 1; columns <= Math.max(1, count); columns += 1) {
    const rows = Math.max(1, Math.ceil(count / columns));
    const cellWidth = (innerWidth - gap * (columns - 1)) / columns;
    const cellHeight = (innerHeight - gap * (rows - 1)) / rows;
    if (cellWidth <= 0 || cellHeight <= 0) continue;
    // Largeur d'une photo 3:2 posée dans la case, sans la déborder.
    const photoWidth = Math.min(cellWidth, cellHeight * PHOTO_ASPECT);
    if (photoWidth > bestPhotoWidth) {
      bestPhotoWidth = photoWidth;
      best = { columns, rows, cellWidth, cellHeight, gap };
    }
  }
  return best;
}

/** Vrai si une case est trop grande pour la vignette : il faut l'aperçu. */
export function needsPreview(
  layout: SurveyLayout,
  devicePixelRatio: number,
): boolean {
  const ratio = devicePixelRatio > 0 ? devicePixelRatio : 1;
  return (
    Math.max(layout.cellWidth, layout.cellHeight) * ratio > THUMBNAIL_PIXELS
  );
}

/** Déplacement de la photo active au clavier. */
export type SurveyMove = "previous" | "next" | "above" | "below";

/**
 * Rang de la photo voisine de celle du rang `index` : une photo avant ou
 * après, ou une rangée au-dessus ou en dessous ; `undefined` au bord.
 */
export function neighborIndex(
  index: number,
  count: number,
  columns: number,
  move: SurveyMove,
): number | undefined {
  const delta = {
    previous: -1,
    next: 1,
    above: -columns,
    below: columns,
  }[move];
  const target = index + delta;
  return target >= 0 && target < count ? target : undefined;
}

/**
 * Photo qui devient active quand la photo active quitte l'Ensemble : la
 * suivante, sinon la précédente ; `null` s'il n'en reste aucune.
 */
export function activeAfterRemoval(
  shownIds: readonly number[],
  removedId: number,
): number | null {
  const index = shownIds.indexOf(removedId);
  if (index === -1) return null;
  return shownIds[index + 1] ?? shownIds[index - 1] ?? null;
}

/**
 * Plages de positions consécutives, pour charger d'un coup des photos
 * voisines dans la grille : [1, 2, 3, 7] → [[1, 3], [7, 7]].
 */
export function positionRanges(
  positions: readonly number[],
): [number, number][] {
  const ranges: [number, number][] = [];
  for (const position of positions) {
    const last = ranges.at(-1);
    if (last !== undefined && position === last[1] + 1) last[1] = position;
    else ranges.push([position, position]);
  }
  return ranges;
}
