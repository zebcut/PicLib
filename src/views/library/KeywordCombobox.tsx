import { useEffect, useId, useState } from "react";
import type { KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";

import { suggestKeywords } from "../../services/keywords";
import type { KeywordSuggestionDto } from "../../services/keywords";
import styles from "./KeywordCombobox.module.css";

/** Délai avant de chercher des suggestions, en millisecondes. */
const SUGGEST_DELAY_MS = 80;

/**
 * Champ de texte avec autocomplétion des mots-clés : ↑ ↓
 * choisissent une suggestion, Tab ou Entrée la prennent (son chemin complet
 * remplace le mot-clé en cours), Entrée sans suggestion choisie appelle
 * `onEnter`, Échap ferme la liste.
 *
 * @param multiple plusieurs mots-clés séparés par des virgules : seul le
 *   dernier (le mot-clé en cours) est complété.
 * @param onChange nouveau texte, avec la suggestion prise s'il vient d'en
 *   prendre une (`null` s'il a été tapé).
 */
export function KeywordCombobox({
  value,
  onChange,
  onEnter,
  multiple,
  label,
  placeholder,
  title,
  describedBy,
  autoFocus,
}: {
  value: string;
  onChange: (text: string, picked: KeywordSuggestionDto | null) => void;
  onEnter?: () => void;
  multiple: boolean;
  label: string;
  placeholder?: string;
  title?: string;
  describedBy?: string;
  autoFocus?: boolean;
}) {
  const { t } = useTranslation();
  const listId = useId();
  // Suggestions reçues, avec le mot-clé en cours qu'elles concernent : une
  // réponse tardive n'est jamais montrée pour un autre texte.
  const [fetched, setFetched] = useState<{
    token: string;
    list: KeywordSuggestionDto[];
  } | null>(null);
  const [highlight, setHighlight] = useState<{
    token: string;
    index: number;
  } | null>(null);
  // Mot-clé en cours pour lequel la liste a été fermée (Échap, suggestion
  // prise) : elle revient dès qu'il change.
  const [dismissed, setDismissed] = useState<string | null>(null);

  const tokenOf = (text: string) => (multiple ? currentToken(text) : text);
  const token = tokenOf(value);
  useEffect(() => {
    if (token.trim() === "") return;
    let isActive = true;
    const timer = setTimeout(() => {
      suggestKeywords(token).then(
        (list) => {
          if (isActive) setFetched({ token, list });
        },
        (error: unknown) => {
          console.error("Failed to suggest keywords", error);
        },
      );
    }, SUGGEST_DELAY_MS);
    return () => {
      isActive = false;
      clearTimeout(timer);
    };
  }, [token]);

  const suggestions =
    fetched?.token === token && dismissed !== token && token.trim() !== ""
      ? fetched.list
      : [];
  const highlighted = highlight?.token === token ? highlight.index : null;

  const accept = (suggestion: KeywordSuggestionDto) => {
    const path = [...suggestion.parents, suggestion.name];
    const next = multiple ? replaceToken(value, path) : path.join(" > ");
    onChange(next, suggestion);
    setDismissed(tokenOf(next));
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    const count = suggestions.length;
    switch (event.key) {
      case "ArrowDown":
        if (count === 0) return;
        event.preventDefault();
        setHighlight({
          token,
          index:
            highlighted === null ? 0 : Math.min(highlighted + 1, count - 1),
        });
        return;
      case "ArrowUp":
        if (count === 0 || highlighted === null) return;
        event.preventDefault();
        setHighlight({ token, index: Math.max(highlighted - 1, 0) });
        return;
      case "Tab": {
        const suggestion = suggestions[highlighted ?? 0];
        if (suggestion === undefined) return;
        event.preventDefault();
        accept(suggestion);
        return;
      }
      case "Enter": {
        event.preventDefault();
        const suggestion =
          highlighted === null ? undefined : suggestions[highlighted];
        if (suggestion === undefined) onEnter?.();
        else accept(suggestion);
        return;
      }
      case "Escape":
        if (count === 0) return;
        // La liste se ferme ; la fenêtre qui contient le champ reste.
        event.preventDefault();
        event.stopPropagation();
        setDismissed(token);
        return;
    }
  };

  return (
    <div className={styles.combobox}>
      <input
        className={styles.input}
        type="text"
        role="combobox"
        aria-label={label}
        aria-describedby={describedBy}
        aria-autocomplete="list"
        aria-expanded={suggestions.length > 0}
        aria-controls={listId}
        placeholder={placeholder}
        title={title}
        spellCheck={false}
        autoFocus={autoFocus}
        value={value}
        onChange={(event) => {
          onChange(event.target.value, null);
        }}
        onKeyDown={onKeyDown}
      />
      {suggestions.length > 0 && (
        <ul
          id={listId}
          className={styles.suggestions}
          role="listbox"
          aria-label={t("keywords.suggestions")}
        >
          {suggestions.map((suggestion, index) => (
            <li
              key={suggestion.id}
              role="option"
              aria-selected={index === highlighted}
              className={styles.suggestion}
              // `mousedown` et non `click` : le champ garde le focus.
              onMouseDown={(event) => {
                event.preventDefault();
                accept(suggestion);
              }}
            >
              {suggestionLabel(suggestion)}
            </li>
          ))}
        </ul>
      )}
    </div>
  );
}

/** Mot-clé en cours : ce qui suit la dernière virgule. */
export function currentToken(text: string): string {
  return text.slice(text.lastIndexOf(",") + 1);
}

/**
 * Remplace le mot-clé en cours par le chemin complet d'une suggestion :
 * `Vacances, Ora` devient `Vacances, Fruits > Orange`.
 */
export function replaceToken(text: string, path: readonly string[]): string {
  const comma = text.lastIndexOf(",");
  const before = comma === -1 ? "" : `${text.slice(0, comma + 1)} `;
  return `${before}${path.join(" > ")}`;
}

/** « Orange (Fruits) » quand le nom existe à plusieurs endroits. */
function suggestionLabel(suggestion: KeywordSuggestionDto): string {
  return suggestion.ambiguous && suggestion.parents.length > 0
    ? `${suggestion.name} (${suggestion.parents.join(" > ")})`
    : suggestion.name;
}
