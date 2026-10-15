import { useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { PanelSection } from "../../components/PanelSection";
import { Tree } from "../../components/Tree";
import type { KeywordListItemDto } from "../../services/keywordList";
import type { SelectionKeywordDto } from "../../services/keywords";
import type { KeywordRefusal } from "../../state/useCatalogEdits";
import { selectionPresence } from "../../state/useKeywordPanel";
import type { KeywordPanelState } from "../../state/useKeywordPanel";
import type { KeywordSets } from "../../state/useKeywordSets";
import type { SuggestedKeywordsState } from "../../state/useSuggestedKeywords";
import { useStoredIdSet } from "../../state/useStoredIdSet";
import { buildTree, visibleRows } from "../../utils/tree";
import { KeywordEntry } from "./KeywordEntry";
import { KeywordSetSection } from "./KeywordSetSection";
import { KeywordSuggestionSection } from "./KeywordSuggestionSection";
import styles from "./KeywordPanel.module.css";

/** Clé sous laquelle l'état replié de la section est retenu. */
const SECTION_STORAGE_KEY = "piclib.panel.keywords.open";

/** Clé des branches repliées de l'arbre (dépliées par défaut). */
const COLLAPSED_STORAGE_KEY = "piclib.panel.keywords.collapsed";

/**
 * Modes d'affichage : l'arbre des mots-clés et de leurs parents, ou la
 * liste à plat de ceux qui seront écrits dans les fichiers,
 * intitulée « Mots-clés ».
 */
const MODES = ["tree", "exported"] as const;
type Mode = (typeof MODES)[number];

/**
 * Panneau « Mots-clés » : les mots-clés des photos
 * sélectionnées en arbre, chacun sous son parent ; un parent seulement
 * impliqué (pas saisi) est atténué, un mot-clé qui n'est pas sur toutes
 * les photos est marqué d'une étoile, un × retire un mot-clé saisi. Le
 * champ en dessous en ajoute ; les suggestions et le jeu de
 * mots-clés les posent en un clic.
 *
 * @param keywords tous les mots-clés (noms des suggestions et des cases du
 *   jeu).
 * @param onApplyKeyword pose une suggestion sur la sélection.
 * @param onToggleKeyword pose un mot-clé du jeu sur la sélection, ou l'en
 *   retire s'il est sur toutes les photos.
 */
export function KeywordPanel({
  selectedIds,
  state,
  sets,
  suggestions,
  keywords,
  onAdd,
  onRemove,
  onApplyKeyword,
  onToggleKeyword,
}: {
  selectedIds: ReadonlySet<number>;
  state: KeywordPanelState;
  sets: KeywordSets;
  suggestions: SuggestedKeywordsState;
  keywords: readonly KeywordListItemDto[];
  onAdd: (mediaIds: number[], entry: string) => Promise<KeywordRefusal | null>;
  onRemove: (mediaIds: number[], keywordId: number) => void;
  onApplyKeyword: (keywordId: number) => void;
  onToggleKeyword: (keywordId: number) => void;
}) {
  const { t } = useTranslation();
  const [mode, setMode] = useState<Mode>("tree");
  const presence = useMemo(() => selectionPresence(state), [state]);
  const byId = useMemo(
    () => new Map(keywords.map((keyword) => [keyword.id, keyword])),
    [keywords],
  );

  const content = () => {
    switch (state.status) {
      case "empty":
        return <p className={styles.message}>{t("metadata.noActive")}</p>;
      case "loading":
        return null;
      case "failed":
        return (
          <p className={styles.message} role="alert">
            {t("keywords.loadFailed")}
          </p>
        );
      case "ready":
        return (
          <>
            <select
              className={styles.mode}
              aria-label={t("keywords.mode")}
              value={mode}
              onChange={(event) => {
                setMode(event.target.value as Mode);
              }}
            >
              {MODES.map((value) => (
                <option key={value} value={value}>
                  {t(`keywords.modes.${value}`)}
                </option>
              ))}
            </select>
            {mode === "tree" ? (
              <SelectionTree
                keywords={state.panel.withParents}
                onRemove={(keywordId) => {
                  onRemove([...selectedIds], keywordId);
                }}
              />
            ) : (
              <ExportedList
                keywords={state.panel.exported}
                withParents={state.panel.withParents}
                onRemove={(keywordId) => {
                  onRemove([...selectedIds], keywordId);
                }}
              />
            )}
            <KeywordEntry
              onSubmit={(entry) => onAdd([...selectedIds], entry)}
            />
          </>
        );
    }
  };

  return (
    <PanelSection title={t("keywords.title")} storageKey={SECTION_STORAGE_KEY}>
      {content()}
      <KeywordSuggestionSection
        suggestions={suggestions}
        byId={byId}
        presence={presence}
        canApply={selectedIds.size > 0}
        onApply={onApplyKeyword}
      />
      <KeywordSetSection
        sets={sets}
        keywords={keywords}
        presence={presence}
        canApply={selectedIds.size > 0}
        onToggle={onToggleKeyword}
      />
    </PanelSection>
  );
}

function SelectionTree({
  keywords,
  onRemove,
}: {
  keywords: SelectionKeywordDto[];
  onRemove: (keywordId: number) => void;
}) {
  const { t } = useTranslation();
  const [collapsed, setCollapsed] = useStoredIdSet(COLLAPSED_STORAGE_KEY);
  const tree = useMemo(() => buildTree(keywords), [keywords]);
  const rows = visibleRows(tree, (id) => !collapsed.has(id));

  if (keywords.length === 0) {
    return <p className={styles.message}>{t("keywords.none")}</p>;
  }
  return (
    <div className={styles.tree}>
      <Tree
        label={t("keywords.title")}
        rows={rows}
        onToggle={(ids, expanded) => {
          setCollapsed(ids, !expanded);
        }}
        rowProps={(row) => ({
          className:
            row.node.item.entered === "absent" ? styles.implied : undefined,
        })}
        renderRow={({ node: { item: keyword } }) => (
          <>
            <RemoveButton keyword={keyword} onRemove={onRemove} />
            <KeywordName keyword={keyword} />
          </>
        )}
      />
    </div>
  );
}

/**
 * Ce qui sera écrit dans les fichiers, à plat, comme dans Lightroom : un
 * mot-clé par ligne. Un × retire des photos sélectionnées un mot-clé saisi ;
 * un parent seulement impliqué, atténué, n'en a pas : il part avec ses
 * enfants.
 */
function ExportedList({
  keywords,
  withParents,
  onRemove,
}: {
  keywords: SelectionKeywordDto[];
  /** Tous les mots-clés de la sélection avec leurs parents, pour l'ordre. */
  withParents: SelectionKeywordDto[];
  onRemove: (keywordId: number) => void;
}) {
  const { t } = useTranslation();
  // Ordre de la hiérarchie : chaque parent, puis ses
  // enfants, chaque niveau par ordre alphabétique, comme dans l'arbre.
  const ordered = useMemo(() => {
    const order = new Map(
      visibleRows(buildTree(withParents), () => true).map((row, index) => [
        row.node.item.id,
        index,
      ]),
    );
    const rank = (id: number) => order.get(id) ?? Number.MAX_SAFE_INTEGER;
    return [...keywords].sort((a, b) => rank(a.id) - rank(b.id));
  }, [keywords, withParents]);
  if (keywords.length === 0) {
    return <p className={styles.message}>{t("keywords.none")}</p>;
  }
  return (
    <ul className={styles.flatList} aria-label={t("keywords.exportedList")}>
      {ordered.map((keyword) => (
        <li
          key={keyword.id}
          className={
            keyword.entered === "absent"
              ? [styles.flatItem, styles.implied].join(" ")
              : styles.flatItem
          }
        >
          <RemoveButton keyword={keyword} onRemove={onRemove} />
          <KeywordName keyword={keyword} />
        </li>
      ))}
    </ul>
  );
}

/**
 * Nom d'un mot-clé, son chemin au survol, une étoile s'il est partiel ; en
 * gras s'il a été saisi, pour mieux le repérer.
 */
function KeywordName({ keyword }: { keyword: SelectionKeywordDto }) {
  const { t } = useTranslation();
  return (
    <span
      className={
        keyword.entered === "absent"
          ? styles.name
          : [styles.name, styles.entered].join(" ")
      }
      title={[...keyword.parents, keyword.name].join(" > ")}
    >
      {keyword.name}
      {!keyword.onAll && (
        <span className={styles.partial} title={t("keywords.partial")}>
          {" *"}
        </span>
      )}
    </span>
  );
}

/**
 * × devant le nom, qui retire un mot-clé saisi des photos sélectionnées
 * ; un espace vide de même largeur pour un parent
 * seulement impliqué, afin que les noms restent alignés.
 */
function RemoveButton({
  keyword,
  onRemove,
}: {
  keyword: SelectionKeywordDto;
  onRemove: (keywordId: number) => void;
}) {
  const { t } = useTranslation();
  if (keyword.entered === "absent") {
    return <span className={styles.removeSpace} aria-hidden />;
  }
  return (
    <button
      type="button"
      className={styles.remove}
      aria-label={t("keywords.remove", { name: keyword.name })}
      title={t("keywords.remove", { name: keyword.name })}
      onClick={() => {
        onRemove(keyword.id);
      }}
    >
      ×
    </button>
  );
}
