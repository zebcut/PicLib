import "i18next";

import type fr from "./locales/fr.json";

// Déclare la structure des textes à TypeScript : une clé de traduction
// inexistante (`t("welcome.titel")`) devient une erreur de compilation. Le
// français sert de référence ; un test vérifie que l'anglais a les mêmes clés.
declare module "i18next" {
  interface CustomTypeOptions {
    defaultNS: "translation";
    resources: { translation: typeof fr };
  }
}
