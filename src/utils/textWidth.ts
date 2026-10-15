/**
 * Largeur d'un texte à l'écran, sans l'afficher : pour savoir jusqu'où une
 * liste virtualisée doit défiler horizontalement, alors que la plupart de
 * ses lignes ne sont pas dessinées.
 */

/** Largeur moyenne d'un caractère, en fraction de la taille de la police. */
const AVERAGE_CHAR_EM = 0.6;

/** Au-delà, les largeurs retenues sont oubliées (elles se recalculent). */
const MAX_CACHED = 100_000;

/** Police : sa description CSS complète et sa taille en pixels. */
export interface FontSpec {
  css: string;
  sizePx: number;
}

let context: OffscreenCanvasRenderingContext2D | null | undefined;
const cache = new Map<string, number>();

/**
 * Police de l'interface (`--font-family-ui`) à la taille donnée par la
 * variable CSS `sizeVariable` (`--font-size-small`…), lue dans le thème.
 */
export function uiFont(sizeVariable: string, fallbackPx: number): FontSpec {
  const style = getComputedStyle(document.documentElement);
  const family = style.getPropertyValue("--font-family-ui").trim();
  const size = Number.parseFloat(style.getPropertyValue(sizeVariable));
  const sizePx = Number.isFinite(size) ? size : fallbackPx;
  return {
    css: `${String(sizePx)}px ${family === "" ? "sans-serif" : family}`,
    sizePx,
  };
}

/**
 * Largeur en pixels de `text` écrit avec `font`. Chaque largeur est
 * calculée une fois. Sans moyen de mesurer (environnement de test), une
 * estimation d'après le nombre de caractères.
 */
export function textWidth(text: string, font: FontSpec): number {
  const key = `${font.css}\u0000${text}`;
  const known = cache.get(key);
  if (known !== undefined) return known;
  const measure = canvasContext();
  let width: number;
  if (measure === null) {
    width = text.length * font.sizePx * AVERAGE_CHAR_EM;
  } else {
    measure.font = font.css;
    width = measure.measureText(text).width;
  }
  if (cache.size >= MAX_CACHED) cache.clear();
  cache.set(key, width);
  return width;
}

function canvasContext(): OffscreenCanvasRenderingContext2D | null {
  if (context === undefined) {
    context =
      typeof OffscreenCanvas === "undefined"
        ? null
        : new OffscreenCanvas(1, 1).getContext("2d");
  }
  return context;
}
