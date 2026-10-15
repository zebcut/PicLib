/** Langues de l'interface. Ajouter une langue = l'ajouter ici + son fichier. */
export const SUPPORTED_LANGUAGES = ["fr", "en"] as const;

/** Code d'une langue prise en charge par l'interface. */
export type SupportedLanguage = (typeof SUPPORTED_LANGUAGES)[number];

/**
 * Langue utilisée quand aucune des langues préférées du système n'est prise
 * en charge : l'anglais est la plus largement comprise.
 */
export const FALLBACK_LANGUAGE: SupportedLanguage = "en";

function isSupportedLanguage(code: string): code is SupportedLanguage {
  return (SUPPORTED_LANGUAGES as readonly string[]).includes(code);
}

/**
 * Choisit la langue de l'interface à partir des langues préférées du système,
 * par ordre de préférence (par exemple `navigator.languages`).
 *
 * Seule la langue principale compte : `fr-CA` et `fr-FR` donnent `fr`.
 */
export function detectLanguage(
  preferredLanguages: readonly string[],
): SupportedLanguage {
  for (const tag of preferredLanguages) {
    const primarySubtag = tag.split("-")[0]?.toLowerCase() ?? "";
    if (isSupportedLanguage(primarySubtag)) {
      return primarySubtag;
    }
  }
  return FALLBACK_LANGUAGE;
}
