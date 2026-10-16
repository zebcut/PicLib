import i18next from "i18next";

/** Unités de taille, du plus petit au plus grand (multiples de 1000). */
const SIZE_UNITS = [
  "byte",
  "kilobyte",
  "megabyte",
  "gigabyte",
  "terabyte",
  "petabyte",
] as const;

/**
 * Formate un nombre entier selon la langue (« 1 234 » en français, « 1,234 »
 * en anglais).
 *
 * @param language langue à utiliser ; par défaut, celle de l'interface.
 */
export function formatCount(
  value: number,
  language: string = i18next.language,
): string {
  return new Intl.NumberFormat(language, { maximumFractionDigits: 0 }).format(
    value,
  );
}

/**
 * Formate une taille en octets avec l'unité la plus lisible, en multiples de
 * 1000 comme le Finder de macOS (« 1,2 Go » en français, « 1.2 GB » en
 * anglais).
 *
 * @param language langue à utiliser ; par défaut, celle de l'interface.
 */
export function formatBytes(
  bytes: number,
  language: string = i18next.language,
): string {
  let value = bytes;
  let unitIndex = 0;
  while (value >= 1000 && unitIndex < SIZE_UNITS.length - 1) {
    value /= 1000;
    unitIndex += 1;
  }
  return new Intl.NumberFormat(language, {
    style: "unit",
    unit: SIZE_UNITS[unitIndex],
    // Pas de décimale pour les octets, une seule au-delà.
    maximumFractionDigits: unitIndex === 0 ? 0 : 1,
    // « 512 octets » plutôt que l'abréviation « 512 o », peu parlante.
    unitDisplay: unitIndex === 0 ? "long" : "short",
  }).format(value);
}

/**
 * Options de traduction d'un nombre : `count` pour l'accord (« 1 photo »,
 * « 2 photos ») et `formatted` pour l'affichage selon la langue.
 */
export function counted(value: number): { count: number; formatted: string } {
  return { count: value, formatted: formatCount(value) };
}

/**
 * Formats a ratio (0 to 1) as a whole percentage, rounded down so that 100 %
 * only shows once everything is done ("42 %" in French, "42%" in English).
 *
 * @param language langue à utiliser ; par défaut, celle de l'interface.
 */
export function formatPercent(
  ratio: number,
  language: string = i18next.language,
): string {
  return new Intl.NumberFormat(language, {
    style: "percent",
    maximumFractionDigits: 0,
  }).format(Math.floor(ratio * 100) / 100);
}

/**
 * Values for a "{{done}} of {{total}}" message: `count` picks the plural,
 * `done` and `total` are formatted in the interface language.
 */
export function progressCounts(
  done: number,
  total: number,
): { count: number; done: string; total: string } {
  return { count: done, done: formatCount(done), total: formatCount(total) };
}

/**
 * Formate une durée de vidéo comme un lecteur : « 0:07 », « 12:34 »,
 * « 1:02:03 ». Les secondes sont arrondies à l'inférieur.
 */
export function formatDuration(milliseconds: number): string {
  const totalSeconds = Math.max(0, Math.floor(milliseconds / 1000));
  const hours = Math.floor(totalSeconds / 3600);
  const minutes = Math.floor((totalSeconds % 3600) / 60);
  const seconds = String(totalSeconds % 60).padStart(2, "0");
  return hours > 0
    ? `${String(hours)}:${String(minutes).padStart(2, "0")}:${seconds}`
    : `${String(minutes)}:${seconds}`;
}

/**
 * Formate une date de prise de vue locale (« 2024-07-14T18:30:05 ») selon la
 * langue, telle quelle : l'heure affichée est celle de l'appareil, sans
 * conversion de fuseau horaire.
 *
 * @param language langue à utiliser ; par défaut, celle de l'interface.
 */
export function formatCaptureTime(
  localTime: string,
  language: string = i18next.language,
): string {
  // Lue comme une heure UTC et affichée en UTC : aucun décalage n'est
  // appliqué, pas même celui d'un changement d'heure.
  const date = new Date(`${localTime}Z`);
  if (Number.isNaN(date.getTime())) return localTime;
  return new Intl.DateTimeFormat(language, {
    dateStyle: "medium",
    timeStyle: "short",
    timeZone: "UTC",
  }).format(date);
}

/**
 * Formate un jour (« 2026-09-24 ») selon la langue : « 24 sept. 2026 ».
 *
 * @param language langue à utiliser ; par défaut, celle de l'interface.
 */
export function formatDay(
  isoDate: string,
  language: string = i18next.language,
): string {
  const date = new Date(`${isoDate}T00:00:00Z`);
  if (Number.isNaN(date.getTime())) return isoDate;
  return new Intl.DateTimeFormat(language, {
    dateStyle: "medium",
    timeZone: "UTC",
  }).format(date);
}

/**
 * Formate un instant (millisecondes depuis 1970) dans le fuseau de
 * l'ordinateur, selon la langue : « 13 oct. 2026, 17:10 ».
 *
 * @param language langue à utiliser ; par défaut, celle de l'interface.
 */
export function formatInstant(
  milliseconds: number,
  language: string = i18next.language,
): string {
  return new Intl.DateTimeFormat(language, {
    dateStyle: "medium",
    timeStyle: "short",
  }).format(new Date(milliseconds));
}
