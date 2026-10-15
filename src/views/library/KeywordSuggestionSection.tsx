import { useTranslation } from "react-i18next";

import type { KeywordListItemDto } from "../../services/keywordList";
import type { KeywordPresenceDto } from "../../services/keywords";
import type { SuggestedKeywordsState } from "../../state/useSuggestedKeywords";
import { itemPath } from "../../utils/tree";
import shared from "./KeywordSection.module.css";

/**
 * Section « Suggestions de mots-clés » du panneau Mots-clés :
 * neuf mots-clés probables pour la photo active (ceux des photos prises au
 * même moment, ceux qui vont avec les siens, les récents). Un clic pose le
 * mot-clé sur toute la sélection.
 *
 * @param byId tous les mots-clés, par identifiant (noms et chemins).
 * @param presence présence des mots-clés sur la sélection.
 * @param canApply faux sans photo sélectionnée.
 */
export function KeywordSuggestionSection({
  suggestions,
  byId,
  presence,
  canApply,
  onApply,
}: {
  suggestions: SuggestedKeywordsState;
  byId: ReadonlyMap<number, KeywordListItemDto>;
  presence: ReadonlyMap<number, KeywordPresenceDto>;
  canApply: boolean;
  onApply: (keywordId: number) => void;
}) {
  const { t } = useTranslation();
  const keywords =
    suggestions.status === "ready"
      ? suggestions.ids.flatMap((id) => byId.get(id) ?? [])
      : [];

  return (
    <div className={shared.section}>
      <div className={shared.header}>
        <span className={shared.label}>{t("keywordSuggestions.title")}</span>
      </div>
      {suggestions.status === "failed" && (
        <p className={shared.hint} role="alert">
          {t("keywordSuggestions.loadFailed")}
        </p>
      )}
      {suggestions.status === "ready" && keywords.length === 0 && (
        <p className={shared.hint}>{t("keywordSuggestions.none")}</p>
      )}
      {keywords.length > 0 && (
        <div
          className={shared.grid}
          role="group"
          aria-label={t("keywordSuggestions.title")}
        >
          {keywords.map((keyword) => {
            const state = presence.get(keyword.id) ?? "absent";
            const label = t("keywordSuggestions.apply", {
              path: itemPath(byId, keyword.id),
            });
            return (
              <button
                key={keyword.id}
                type="button"
                className={[
                  shared.slot,
                  state === "partial" ? shared.partial : undefined,
                ]
                  .join(" ")
                  .trim()}
                disabled={!canApply}
                title={label}
                aria-label={label}
                onClick={() => {
                  onApply(keyword.id);
                }}
              >
                <span className={shared.slotName}>{keyword.name}</span>
              </button>
            );
          })}
        </div>
      )}
    </div>
  );
}
