/**
 * Zoom de la Loupe : photo adaptée à la fenêtre, ou à 100 % (un pixel de la
 * photo par pixel de l'écran) centrée sur un point de la photo.
 */
export type Zoom =
  | { mode: "fit" }
  | {
      mode: "full";
      /** Point à centrer, de 0 (bord gauche) à 1 (bord droit). */
      focusX: number;
      /** Point à centrer, de 0 (haut) à 1 (bas). */
      focusY: number;
    };

/** Photo adaptée à la fenêtre. */
export const FIT: Zoom = { mode: "fit" };

/**
 * 100 %, centré sur le milieu de la photo. Chaque passage à 100 % est un
 * nouvel objet : l'état de chargement de la pleine résolution y est attaché.
 */
export function fullCentered(): Zoom {
  return { mode: "full", focusX: 0.5, focusY: 0.5 };
}

/**
 * Point cliqué dans une image affichée, de 0 à 1 sur chaque axe (borné si
 * le clic tombe sur le bord).
 */
export function focusAt(
  rect: Pick<DOMRect, "left" | "top" | "width" | "height">,
  clientX: number,
  clientY: number,
): Zoom {
  const ratio = (value: number, size: number) =>
    size > 0 ? Math.min(1, Math.max(0, value / size)) : 0.5;
  return {
    mode: "full",
    focusX: ratio(clientX - rect.left, rect.width),
    focusY: ratio(clientY - rect.top, rect.height),
  };
}

/**
 * Taille d'affichage à 100 %, en pixels CSS : un pixel de la photo par
 * pixel physique de l'écran (deux pixels physiques par pixel CSS sur un
 * écran Retina).
 */
export function fullSizeInCssPixels(
  naturalWidth: number,
  naturalHeight: number,
  devicePixelRatio: number,
): { width: number; height: number } {
  const ratio = devicePixelRatio > 0 ? devicePixelRatio : 1;
  return { width: naturalWidth / ratio, height: naturalHeight / ratio };
}

/**
 * Position de défilement qui place le point `focus` (0 à 1) au centre de la
 * partie visible, sans sortir de l'image.
 */
export function scrollToCenter(
  focus: number,
  contentSize: number,
  viewportSize: number,
): number {
  const wanted = focus * contentSize - viewportSize / 2;
  return Math.min(Math.max(0, wanted), Math.max(0, contentSize - viewportSize));
}

/**
 * Point de la photo au centre de la partie visible, de 0 à 1 : l'inverse de
 * `scrollToCenter`, pour qu'une autre photo montre le même endroit.
 */
export function centerFocus(
  scroll: number,
  viewportSize: number,
  contentSize: number,
): number {
  if (contentSize <= 0) return 0.5;
  return Math.min(1, Math.max(0, (scroll + viewportSize / 2) / contentSize));
}
