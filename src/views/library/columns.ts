/**
 * Largeur des colonnes latérales de la Bibliothèque, réglée en
 * glissant leur bord et retenue d'une session à l'autre, en pixels.
 */
export const COLUMN_WIDTH = {
  left: {
    storageKey: "piclib.column.left.width",
    default: 220,
    min: 180,
    max: 480,
  },
  right: {
    storageKey: "piclib.column.right.width",
    default: 280,
    min: 240,
    max: 800,
  },
} as const;
