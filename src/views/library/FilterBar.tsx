import { useEffect, useEffectEvent, useId, useState } from "react";
import type { MouseEvent, ReactNode } from "react";
import { useTranslation } from "react-i18next";

import type {
  GridColorChoice,
  GridFilter,
  GridFlagChoice,
  GridKindChoice,
  GridRatingComparison,
  GridSource,
  GridTextField,
  GridTextRule,
  GridTextSearch,
} from "../../services/library";
import type {
  FilterPresetOperation,
  FilterPresetRefusal,
} from "../../services/filterPresets";
import { isApplePlatform } from "../../shortcuts/registry";
import { applyPreset } from "../../state/filterPresets";
import {
  activeFilter,
  changeFilter,
  chooseTab,
  EMPTY_TEXT_SEARCH,
  isFiltering,
  NO_FILTER_BAR,
  toggleEnabled,
  toggleIn,
} from "../../state/gridFilter";
import type { FilterBarState, FilterTab } from "../../state/gridFilter";
import { useStoredColumns } from "../../state/useStoredColumns";
import styles from "./FilterBar.module.css";
import { MetadataBrowser } from "./MetadataBrowser";
import { PresetMenu } from "./PresetMenu";

const FLAGS: readonly GridFlagChoice[] = ["picked", "unflagged", "rejected"];
const COMPARISONS: readonly GridRatingComparison[] = [
  "atLeast",
  "equal",
  "atMost",
];
const STARS = [1, 2, 3, 4, 5] as const;
const COLORS: readonly GridColorChoice[] = [
  "red",
  "yellow",
  "green",
  "blue",
  "purple",
  "none",
];
const KINDS: readonly GridKindChoice[] = ["photo", "video", "raw", "stack"];
const TEXT_FIELDS: readonly GridTextField[] = [
  "all",
  "fileName",
  "title",
  "caption",
  "keywords",
  "place",
];
const TEXT_RULES: readonly GridTextRule[] = [
  "contains",
  "containsAll",
  "containsWords",
  "doesNotContain",
  "startsWith",
  "endsWith",
];
/** Délai après la dernière frappe avant de filtrer, en millisecondes. */
const TYPING_DELAY_MS = 300;

/** Change l'état de la barre à partir de l'état le plus récent. */
export type FilterBarUpdate = (
  update: (state: FilterBarState) => FilterBarState,
) => void;

/**
 * Barre de filtres au-dessus de la grille : les onglets Texte,
 * Attribut, Métadonnées et Aucun (Cmd+clic pour en combiner plusieurs), la
 * ligne du filtre Texte, celle du filtre Attribut :
 * marqueur, note, couleur, type, et le navigateur en colonnes du filtre
 * Métadonnées.
 *
 * À droite, les préréglages.
 *
 * @param refreshKey les colonnes du filtre Métadonnées et les préréglages
 *   sont relus quand cette valeur change.
 * @param onChangePreset absent : les préréglages de l'utilisateur ne sont
 *   pas proposés.
 */
export function FilterBar({
  state,
  source,
  refreshKey,
  onChange,
  onChangePreset,
  isLocked,
  onLockChange,
}: {
  state: FilterBarState;
  source: GridSource;
  refreshKey: number;
  onChange: FilterBarUpdate;
  onChangePreset?: (
    operation: FilterPresetOperation,
  ) => Promise<FilterPresetRefusal | null>;
  /** Cadenas fermé : le filtre est gardé en changeant de source. */
  isLocked: boolean;
  onLockChange: (isLocked: boolean) => void;
}) {
  const { t } = useTranslation();
  const isApple = isApplePlatform();
  const [columns, setColumns] = useStoredColumns();

  const tab = (id: FilterTab | "none", label: string) => {
    const isOpen =
      id === "none" ? state.tabs.length === 0 : state.tabs.includes(id);
    return (
      <button
        type="button"
        className={styles.tab}
        aria-pressed={isOpen}
        onClick={(event: MouseEvent) => {
          if (id === "none") {
            onChange(() => NO_FILTER_BAR);
          } else {
            const combine = isApple ? event.metaKey : event.ctrlKey;
            onChange((current) => chooseTab(current, id, combine));
          }
        }}
      >
        {label}
      </button>
    );
  };

  return (
    <section className={styles.bar} aria-label={t("filters.label")}>
      <div className={styles.tabs}>
        <span className={styles.title}>{t("filters.title")}</span>
        {tab("text", t("filters.tabs.text"))}
        {tab("attribute", t("filters.tabs.attribute"))}
        {tab("metadata", t("filters.tabs.metadata"))}
        {tab("none", t("filters.tabs.none"))}
        {!state.enabled && isFiltering(state.filter) && (
          <button
            type="button"
            className={styles.disabled}
            onClick={() => {
              onChange(toggleEnabled);
            }}
          >
            {t("filters.disabled")}
          </button>
        )}
        <button
          type="button"
          className={classes(styles.tab, styles.lock)}
          aria-pressed={isLocked}
          aria-label={t("filters.lock")}
          title={isLocked ? t("filters.locked") : t("filters.unlocked")}
          onClick={() => {
            onLockChange(!isLocked);
          }}
        >
          {isLocked ? "🔒" : "🔓"}
        </button>
        <PresetMenu
          state={state}
          columns={columns}
          refreshKey={refreshKey}
          onApply={(content) => {
            onChange(() => applyPreset(content));
            if (content.tabs.includes("metadata")) setColumns(content.columns);
          }}
          onChangePreset={onChangePreset}
        />
      </div>
      {state.tabs.includes("text") && state.filter.text !== null && (
        <TextFilter
          search={state.filter.text}
          onChange={(patch) => {
            onChange((current) =>
              changeFilter(current, {
                ...current.filter,
                text: {
                  ...(current.filter.text ?? EMPTY_TEXT_SEARCH),
                  ...patch,
                },
              }),
            );
          }}
        />
      )}
      {state.tabs.includes("attribute") && (
        <AttributeFilter
          filter={state.filter}
          onChange={(patch) => {
            onChange((current) =>
              changeFilter(current, { ...current.filter, ...patch }),
            );
          }}
        />
      )}
      {state.tabs.includes("metadata") && (
        <MetadataBrowser
          source={source}
          filter={state.filter}
          countFilter={activeFilter(state)}
          columns={columns}
          onColumnsChange={setColumns}
          refreshKey={refreshKey}
          onChange={(update) => {
            onChange((current) =>
              changeFilter(current, update(current.filter)),
            );
          }}
        />
      )}
    </section>
  );
}

/**
 * Ligne du filtre Texte : champ cherché, règle et saisie, appliquée un
 * court instant après la dernière frappe.
 */
function TextFilter({
  search,
  onChange,
}: {
  search: GridTextSearch;
  onChange: (patch: Partial<GridTextSearch>) => void;
}) {
  const { t } = useTranslation();
  const [draft, setDraft] = useState(search.text);
  const inputId = useId();
  const apply = useEffectEvent((text: string) => {
    onChange({ text });
  });

  useEffect(() => {
    if (draft === search.text) return;
    const timer = setTimeout(() => {
      apply(draft);
    }, TYPING_DELAY_MS);
    return () => {
      clearTimeout(timer);
    };
  }, [draft, search.text]);

  return (
    <div className={styles.attributes}>
      <select
        className={styles.comparison}
        aria-label={t("filters.text.field")}
        value={search.field}
        onChange={(event) => {
          onChange({ field: event.currentTarget.value as GridTextField });
        }}
      >
        {TEXT_FIELDS.map((field) => (
          <option key={field} value={field}>
            {t(`filters.text.fields.${field}`)}
          </option>
        ))}
      </select>
      <select
        className={styles.comparison}
        aria-label={t("filters.text.rule")}
        value={search.rule}
        onChange={(event) => {
          onChange({ rule: event.currentTarget.value as GridTextRule });
        }}
      >
        {TEXT_RULES.map((rule) => (
          <option key={rule} value={rule}>
            {t(`filters.text.rules.${rule}`)}
          </option>
        ))}
      </select>
      <label className={styles.groupLabel} htmlFor={inputId}>
        {t("filters.text.label")}
      </label>
      <input
        id={inputId}
        type="search"
        className={styles.search}
        placeholder={t("filters.text.placeholder")}
        value={draft}
        onChange={(event) => {
          setDraft(event.currentTarget.value);
        }}
      />
    </div>
  );
}

/**
 * Ligne du filtre Attribut, aussi utilisée par le mini-filtre de la
 * filmstrip (sans le type de fichier).
 *
 * @param withKinds propose le type de fichier (photo, vidéo, RAW).
 */
export function AttributeFilter({
  filter,
  onChange,
  withKinds = true,
  className = styles.attributes,
}: {
  filter: GridFilter;
  onChange: (patch: Partial<GridFilter>) => void;
  withKinds?: boolean;
  className?: string;
}) {
  const { t } = useTranslation();
  // Comparaison choisie avant les étoiles.
  const [comparison, setComparison] = useState<GridRatingComparison>(
    filter.rating?.comparison ?? "atLeast",
  );
  const shownComparison = filter.rating?.comparison ?? comparison;

  return (
    <div className={className}>
      <Group label={t("filters.flag.label")}>
        {FLAGS.map((flag) => (
          <Toggle
            key={flag}
            pressed={filter.flags.includes(flag)}
            label={t(`filters.flag.${flag}`)}
            onToggle={() => {
              onChange({ flags: toggleIn(filter.flags, flag) });
            }}
          >
            {t(`filters.flag.${flag}Short`)}
          </Toggle>
        ))}
      </Group>
      <Group label={t("filters.rating.label")}>
        <select
          className={styles.comparison}
          aria-label={t("filters.rating.comparison")}
          value={shownComparison}
          onChange={(event) => {
            const chosen = event.currentTarget.value as GridRatingComparison;
            setComparison(chosen);
            if (filter.rating !== null) {
              onChange({ rating: { ...filter.rating, comparison: chosen } });
            }
          }}
        >
          {COMPARISONS.map((option) => (
            <option key={option} value={option}>
              {t(`filters.rating.${option}`)}
            </option>
          ))}
        </select>
        {STARS.map((stars) => (
          <Toggle
            key={stars}
            className={styles.star}
            pressed={filter.rating !== null && stars <= filter.rating.stars}
            label={t("triage.stars", { count: stars })}
            onToggle={() => {
              onChange({
                // Un second clic sur la même étoile retire la règle.
                rating:
                  filter.rating?.stars === stars
                    ? null
                    : { comparison: shownComparison, stars },
              });
            }}
          >
            ★
          </Toggle>
        ))}
      </Group>
      <Group label={t("filters.color.label")}>
        {COLORS.map((color) => (
          <Toggle
            key={color}
            className={classes(styles.swatch, styles[color])}
            pressed={filter.colors.includes(color)}
            label={
              color === "none" ? t("filters.color.none") : t(`triage.${color}`)
            }
            onToggle={() => {
              onChange({ colors: toggleIn(filter.colors, color) });
            }}
          >
            {color === "none" ? "∅" : ""}
          </Toggle>
        ))}
      </Group>
      {withKinds && (
        <Group label={t("filters.kind.label")}>
          {KINDS.map((kind) => (
            <Toggle
              key={kind}
              pressed={filter.kinds.includes(kind)}
              label={t(`filters.kind.${kind}`)}
              onToggle={() => {
                onChange({ kinds: toggleIn(filter.kinds, kind) });
              }}
            >
              {t(`filters.kind.${kind}`)}
            </Toggle>
          ))}
        </Group>
      )}
    </div>
  );
}

/** Critère du filtre Attribut, avec son titre. */
function Group({ label, children }: { label: string; children: ReactNode }) {
  return (
    <div className={styles.group} role="group" aria-label={label}>
      <span className={styles.groupLabel}>{label}</span>
      {children}
    </div>
  );
}

/** Choix qui se coche et se décoche. */
function Toggle({
  pressed,
  label,
  className,
  onToggle,
  children,
}: {
  pressed: boolean;
  label: string;
  className?: string;
  onToggle: () => void;
  children: ReactNode;
}) {
  return (
    <button
      type="button"
      className={classes(styles.toggle, className)}
      aria-pressed={pressed}
      aria-label={label}
      title={label}
      onClick={onToggle}
    >
      {children}
    </button>
  );
}

/** Classes CSS présentes, séparées par des espaces. */
function classes(...names: (string | undefined)[]): string {
  return names.filter(Boolean).join(" ");
}
