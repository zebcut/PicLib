import i18next from "i18next";
import { initReactI18next } from "react-i18next";

import { detectLanguage, FALLBACK_LANGUAGE } from "./languages";
import type { SupportedLanguage } from "./languages";
import en from "./locales/en.json";
import fr from "./locales/fr.json";

/** Textes de toutes les langues, regroupés dans l'espace de noms par défaut. */
export const resources = {
  fr: { translation: fr },
  en: { translation: en },
} as const;

/**
 * Initialise les traductions dans la langue du système et renvoie la langue
 * retenue. À appeler une seule fois, avant le premier affichage.
 */
export async function initI18n(
  preferredLanguages: readonly string[],
): Promise<SupportedLanguage> {
  const language = detectLanguage(preferredLanguages);
  await i18next.use(initReactI18next).init({
    resources,
    lng: language,
    fallbackLng: FALLBACK_LANGUAGE,
    // React protège déjà l'affichage contre l'injection de HTML : un second
    // échappement afficherait des « &amp; » à l'écran.
    interpolation: { escapeValue: false },
  });
  return language;
}
