/**
 * Affichage d'une image pivotée par l'utilisateur (0, 90, 180 ou 270 degrés
 * dans le sens horaire). Le fichier n'est jamais modifié : seule l'image à
 * l'écran tourne.
 */

/** Vrai pour un quart de tour (90 ou 270°) : largeur et hauteur s'échangent. */
export function isQuarterTurn(rotation: number): boolean {
  return rotation % 180 !== 0;
}

/** Transformation CSS de la rotation ; rien sans rotation. */
export function rotationTransform(rotation: number): string | undefined {
  return rotation === 0 ? undefined : `rotate(${String(rotation)}deg)`;
}

/** Taille occupée à l'écran par une image une fois pivotée. */
export function rotatedSize(
  size: { width: number; height: number },
  rotation: number,
): { width: number; height: number } {
  return isQuarterTurn(rotation)
    ? { width: size.height, height: size.width }
    : size;
}

/**
 * Transformation d'une image centrée par `left: 50%; top: 50%`, pivotée
 * autour de son centre.
 */
export function centeredRotation(rotation: number): string {
  return `translate(-50%, -50%) rotate(${String(rotation)}deg)`;
}
