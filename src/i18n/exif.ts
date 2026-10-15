import type { TFunction } from "i18next";
import i18next from "i18next";

import { formatCount } from "./format";

/**
 * Formate un nombre décimal selon la langue (« 2,8 » en français, « 2.8 »
 * en anglais), avec au plus `maximumFractionDigits` décimales.
 */
function formatDecimal(
  value: number,
  maximumFractionDigits: number,
  language: string,
): string {
  return new Intl.NumberFormat(language, { maximumFractionDigits }).format(
    value,
  );
}

/**
 * Temps de pose, comme sur un appareil photo : « 1/250 s » pour une
 * fraction de seconde, « 2 s » ou « 1,5 s » au-delà.
 */
export function formatExposureTime(
  seconds: number,
  language: string = i18next.language,
): string {
  if (seconds <= 0) return "";
  if (seconds >= 1) return `${formatDecimal(seconds, 1, language)} s`;
  return `1/${formatCount(Math.round(1 / seconds), language)} s`;
}

/** Ouverture : « f/2,8 ». */
export function formatAperture(
  fNumber: number,
  language: string = i18next.language,
): string {
  return `f/${formatDecimal(fNumber, 1, language)}`;
}

/** Focale : « 35 mm », « 4,2 mm ». */
export function formatFocalLength(
  millimeters: number,
  language: string = i18next.language,
): string {
  return `${formatDecimal(millimeters, 1, language)} mm`;
}

/** Dimensions : « 6 000 × 4 000 ». */
export function formatDimensions(
  width: number,
  height: number,
  language: string = i18next.language,
): string {
  return `${formatCount(width, language)} × ${formatCount(height, language)}`;
}

/**
 * Position GPS : « 48,8584° N, 2,2945° E », avec les points cardinaux de
 * la langue (O pour Ouest en français).
 */
export function formatGps(
  t: TFunction,
  latitude: number,
  longitude: number,
  language: string = i18next.language,
): string {
  const latitudeText = `${formatDecimal(Math.abs(latitude), 4, language)}° ${t(
    latitude < 0 ? "metadata.south" : "metadata.north",
  )}`;
  const longitudeText = `${formatDecimal(Math.abs(longitude), 4, language)}° ${t(
    longitude < 0 ? "metadata.west" : "metadata.east",
  )}`;
  return `${latitudeText}, ${longitudeText}`;
}
