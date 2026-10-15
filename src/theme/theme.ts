/** Thèmes de l'interface. */
export const THEMES = ["dark", "light"] as const;

/** Nom d'un thème de l'interface. */
export type ThemeName = (typeof THEMES)[number];

/** Thème au premier lancement, comme dans les logiciels photo classiques. */
export const DEFAULT_THEME: ThemeName = "dark";

/**
 * Applique un thème : les couleurs de `tokens.css` dépendent de l'attribut
 * `data-theme` de l'élément racine.
 */
export function applyTheme(root: HTMLElement, theme: ThemeName): void {
  root.dataset.theme = theme;
}
