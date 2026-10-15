import type { TFunction } from "i18next";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import type {
  KeywordListItemDto,
  KeywordListRefusal,
} from "../../services/keywordList";
import { searchKey } from "../../utils/tree";
import styles from "./KeywordListPanel.module.css";

/** Nombre maximal de mots-clés proposés dans une liste de choix. */
const MAX_CHOICES = 50;

/**
 * Nom à donner : mot-clé à créer ou à renommer, jeu de mots-clés.
 * `onSubmit` renvoie la raison d'un refus, affichée avec `refusalText`, ou
 * `null` (la fenêtre est alors fermée par l'appelant).
 */
export function NameDialog<Refusal>({
  title,
  initialName,
  askExport,
  onSubmit,
  refusalText,
  onClose,
}: {
  title: string;
  initialName: string;
  /** Propose la case « Inclure à l'export » (création d'un mot-clé). */
  askExport: boolean;
  onSubmit: (name: string, includeOnExport: boolean) => Promise<Refusal | null>;
  refusalText: (refusal: Refusal) => string;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorId = useId();
  const [name, setName] = useState(initialName);
  const [includeOnExport, setIncludeOnExport] = useState(true);
  const [refusal, setRefusal] = useState<Refusal | null>(null);
  const [isSaving, setIsSaving] = useState(false);

  const submit = () => {
    if (isSaving || name.trim() === "") return;
    setIsSaving(true);
    onSubmit(name, includeOnExport)
      .then((result) => {
        setRefusal(result);
        setIsSaving(false);
      })
      .catch((error: unknown) => {
        console.error("Failed to save the name", error);
        setIsSaving(false);
      });
  };

  return (
    <Modal
      title={title}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={name.trim() === "" || isSaving}
            onClick={submit}
          >
            {t("common.ok")}
          </Button>
        </>
      }
    >
      <div className={styles.form}>
        <label className={styles.field}>
          {t("keywordList.name")}
          <input
            className={styles.input}
            type="text"
            autoFocus
            spellCheck={false}
            value={name}
            aria-describedby={refusal === null ? undefined : errorId}
            onChange={(event) => {
              setName(event.target.value);
              setRefusal(null);
            }}
            onKeyDown={(event) => {
              if (event.key === "Enter") submit();
              else if (event.key === "Escape") onClose();
            }}
          />
        </label>
        {askExport && (
          <label className={styles.checkboxField}>
            <input
              type="checkbox"
              checked={includeOnExport}
              onChange={(event) => {
                setIncludeOnExport(event.target.checked);
              }}
            />
            {t("keywordList.includeOnExport")}
          </label>
        )}
        {askExport && (
          <p className={styles.hint}>{t("keywordList.includeOnExportHint")}</p>
        )}
        {refusal !== null && (
          <p id={errorId} className={styles.error} role="alert">
            {refusalText(refusal)}
          </p>
        )}
      </div>
    </Modal>
  );
}

/** Choix proposé : un mot-clé, ou la racine (`id` à `null`). */
interface Choice {
  id: number | null;
  label: string;
}

/**
 * Choix d'un autre mot-clé pour `keyword`, hors de sa propre branche : celui
 * dans lequel le fusionner (ses photos et ses sous-mots-clés y passeront),
 * ou celui sous lequel le déplacer (avec, en tête, la racine).
 *
 * @param onChoose renvoie la raison d'un refus, ou `null` (la fenêtre est
 *   alors fermée par l'appelant).
 */
export function ChooseKeywordDialog({
  keyword,
  keywords,
  title,
  explanation,
  actionLabel,
  offerRoot,
  onChoose,
  onClose,
}: {
  keyword: KeywordListItemDto;
  keywords: readonly KeywordListItemDto[];
  title: string;
  explanation: string;
  actionLabel: string;
  /** Propose aussi « la racine » (déplacement). */
  offerRoot: boolean;
  onChoose: (id: number | null) => Promise<KeywordListRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [query, setQuery] = useState("");
  const [chosen, setChosen] = useState<Choice | null>(null);
  const [refusal, setRefusal] = useState<KeywordListRefusal | null>(null);

  // Chemins de tous les mots-clés, hors de la branche de `keyword` (il ne
  // peut ni s'y fusionner ni s'y ranger).
  const choices = useMemo(() => {
    const byId = new Map(keywords.map((item) => [item.id, item]));
    const pathOf = (item: KeywordListItemDto): KeywordListItemDto[] => {
      const path = [item];
      let parentId = item.parentId;
      while (parentId !== null && path.length <= byId.size) {
        const parent = byId.get(parentId);
        if (parent === undefined) break;
        path.unshift(parent);
        parentId = parent.parentId;
      }
      return path;
    };
    // Clé de recherche calculée une fois par chemin, et un seul comparateur
    // pour tout le tri : la liste peut compter des dizaines de milliers de
    // mots-clés.
    const collator = new Intl.Collator();
    return keywords
      .map((item) => ({ item, path: pathOf(item) }))
      .filter(({ path }) => !path.some((step) => step.id === keyword.id))
      .map(({ item, path }) => {
        const label = path.map((step) => step.name).join(" > ");
        return { choice: { id: item.id, label }, key: searchKey(label) };
      })
      .sort((a, b) => collator.compare(a.key, b.key));
  }, [keyword, keywords]);
  const key = searchKey(query.trim());
  const root: Choice = { id: null, label: t("keywordList.root") };
  const shown = [
    ...(offerRoot && key === "" ? [root] : []),
    ...choices
      .filter((entry) => entry.key.includes(key))
      .slice(0, MAX_CHOICES)
      .map((entry) => entry.choice),
  ];

  const choose = (choice: Choice | null) => {
    if (choice === null) return;
    onChoose(choice.id)
      .then(setRefusal)
      .catch((error: unknown) => {
        console.error("Failed to change the keyword", error);
      });
  };

  return (
    <Modal
      title={title}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={chosen === null}
            onClick={() => {
              choose(chosen);
            }}
          >
            {actionLabel}
          </Button>
        </>
      }
    >
      <p>{explanation}</p>
      <input
        className={styles.input}
        type="search"
        autoFocus
        spellCheck={false}
        aria-label={t("keywordList.search")}
        placeholder={t("keywordList.search")}
        value={query}
        onChange={(event) => {
          setQuery(event.target.value);
        }}
        onKeyDown={(event) => {
          if (event.key === "Escape") onClose();
        }}
      />
      <ul
        className={styles.choices}
        role="listbox"
        aria-label={t("keywordList.destination")}
      >
        {shown.map((choice) => (
          <li
            key={choice.id ?? "root"}
            role="option"
            aria-selected={chosen !== null && choice.id === chosen.id}
            className={
              choice.id === null
                ? [styles.choice, styles.rootChoice].join(" ")
                : styles.choice
            }
            onClick={() => {
              setChosen(choice);
              setRefusal(null);
            }}
            onDoubleClick={() => {
              setChosen(choice);
              choose(choice);
            }}
          >
            {choice.label}
          </li>
        ))}
      </ul>
      {refusal !== null && (
        <p className={styles.error} role="alert">
          {listRefusalText(t, refusal)}
        </p>
      )}
    </Modal>
  );
}

/**
 * Confirmation d'une suppression : un mot-clé et sa branche, ou les
 * mots-clés inutilisés. Les photos ne sont jamais supprimées ; la
 * suppression s'annule avec Cmd+Z.
 */
export function DeleteKeywordsDialog({
  title,
  keywordCount,
  photoCount,
  onDelete,
  onClose,
}: {
  title: string;
  keywordCount: number;
  photoCount: number;
  onDelete: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const nothing = keywordCount === 0;
  return (
    <Modal
      title={title}
      actions={
        nothing ? (
          <Button variant="primary" autoFocus onClick={onClose}>
            {t("common.ok")}
          </Button>
        ) : (
          <>
            <Button autoFocus onClick={onClose}>
              {t("common.cancel")}
            </Button>
            <Button variant="primary" onClick={onDelete}>
              {t("keywordList.delete")}
            </Button>
          </>
        )
      }
    >
      {nothing ? (
        <p>{t("keywordList.noUnused")}</p>
      ) : (
        <>
          <p>
            {t("keywordList.deleteKeywords", counted(keywordCount))}{" "}
            {photoCount > 0 &&
              t("keywordList.deletePhotos", counted(photoCount))}
          </p>
          <p className={styles.hint}>{t("keywordList.deleteExplanation")}</p>
        </>
      )}
    </Modal>
  );
}

/** Explication d'un refus, à afficher sous le champ. */
export function listRefusalText(
  t: TFunction,
  refusal: KeywordListRefusal,
): string {
  switch (refusal.kind) {
    case "notFound":
      return t("keywordList.refusals.notFound");
    case "emptyName":
      return t("keywordList.refusals.emptyName");
    case "tooLong":
      return t("keywords.tooLong");
    case "forbiddenCharacter":
      return t("keywordList.refusals.forbidden", { name: refusal.name });
    case "nameTaken":
      return t("keywordList.refusals.nameTaken", { name: refusal.name });
    case "mergeIntoBranch":
      return t("keywordList.refusals.mergeIntoBranch");
    case "moveIntoBranch":
      return t("keywordList.refusals.moveIntoBranch");
  }
}
