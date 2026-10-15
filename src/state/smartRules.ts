/**
 * Règles des collections dynamiques côté interface : la règle
 * par défaut de chaque critère, et la vérification qu'une règle est
 * complète. Fonctions pures.
 */
import type { SmartRule } from "../services/collections";

/** Critères proposés, dans l'ordre du menu (« group » : un sous-groupe). */
export const SMART_CRITERIA = [
  "rating",
  "flag",
  "color",
  "keyword",
  "captureDate",
  "importDate",
  "camera",
  "lens",
  "fileKind",
  "folder",
  "text",
  "gps",
  "stack",
  "group",
] as const satisfies readonly SmartRule["kind"][];

/** Jour d'aujourd'hui (`2024-07-14`), dans le fuseau de l'ordinateur. */
function today(): string {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${String(now.getFullYear())}-${pad(now.getMonth() + 1)}-${pad(now.getDate())}`;
}

/** Règle par défaut d'un critère, à compléter. */
export function defaultRule(kind: SmartRule["kind"]): SmartRule {
  switch (kind) {
    case "group":
      return { kind, combine: "all", rules: [defaultRule("rating")] };
    case "rating":
      return { kind, comparison: "atLeast", stars: 1 };
    case "flag":
      return { kind, flag: "picked" };
    case "color":
      return { kind, color: "red" };
    case "keyword":
      // Les sous-mots-clés aussi, par défaut.
      return { kind, keywordId: 0, includeChildren: true };
    case "captureDate":
    case "importDate":
      return { kind, comparison: "on", date: today() };
    case "camera":
    case "lens":
      return { kind, text: "" };
    case "fileKind":
      return { kind, fileKind: "photo" };
    case "folder":
      return { kind, folderId: 0, includeSubfolders: true };
    case "text":
      return { kind, field: "fileName", rule: "contains", text: "" };
    case "gps":
      return { kind, present: true };
    case "stack":
      return { kind, stacked: true };
  }
}

/** Règles d'une nouvelle collection dynamique : toutes, au moins 1 étoile. */
export function newSmartRules(): SmartRule {
  return defaultRule("group");
}

/**
 * Vrai si la règle est complète : un mot-clé ou un dossier choisi, une date
 * valable.
 */
export function isComplete(rule: SmartRule): boolean {
  switch (rule.kind) {
    case "group":
      return rule.rules.every(isComplete);
    case "keyword":
      return rule.keywordId > 0;
    case "folder":
      return rule.folderId > 0;
    case "captureDate":
    case "importDate":
      return /^\d{4}-\d{2}-\d{2}$/.test(rule.date);
    default:
      return true;
  }
}
