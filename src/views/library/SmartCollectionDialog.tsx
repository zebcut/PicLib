import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import type { CollectionRefusal, SmartRule } from "../../services/collections";
import type { FolderDto } from "../../services/folders";
import type { KeywordListItemDto } from "../../services/keywordList";
import {
  defaultRule,
  isComplete,
  SMART_CRITERIA,
} from "../../state/smartRules";
import { itemPath } from "../../utils/tree";
import { KeywordCombobox } from "./KeywordCombobox";
import listStyles from "./KeywordListPanel.module.css";
import styles from "./SmartCollectionDialog.module.css";

type Group = Extract<SmartRule, { kind: "group" }>;

/** Ce que l'éditeur sait des mots-clés et des dossiers, pour les nommer. */
interface Context {
  keywordPath: (id: number) => string;
  folders: readonly FolderDto[];
}

/**
 * Collection dynamique : son nom (à la création) et ses règles,
 * combinables (toutes, ou au moins une), en groupes imbriqués. Le contenu
 * de la collection se recalcule à chaque lecture.
 *
 * @param name nom à la création ; `null` pour modifier les règles seules.
 * @param onSubmit renvoie la raison d'un refus, ou `null` (la fenêtre est
 *   alors fermée par l'appelant).
 */
export function SmartCollectionDialog({
  title,
  name: initialName,
  rules: initialRules,
  keywords,
  folders,
  refines = null,
  onSubmit,
  onClose,
}: {
  title: string;
  name: string | null;
  rules: SmartRule;
  /** Name of the smart parent whose rules also apply, if any. */
  refines?: string | null;
  keywords: readonly KeywordListItemDto[];
  folders: readonly FolderDto[];
  onSubmit: (
    name: string,
    rules: SmartRule,
  ) => Promise<CollectionRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorId = useId();
  const [name, setName] = useState(initialName ?? "");
  const [rules, setRules] = useState<Group>(
    initialRules.kind === "group"
      ? initialRules
      : { kind: "group", combine: "all", rules: [initialRules] },
  );
  const [refusal, setRefusal] = useState<CollectionRefusal | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const context = useMemo<Context>(() => {
    const byId = new Map(keywords.map((keyword) => [keyword.id, keyword]));
    return {
      keywordPath: (id) => itemPath(byId, id).replaceAll(" > ", " ▸ "),
      folders,
    };
  }, [keywords, folders]);

  const asksName = initialName !== null;
  const canSave =
    !isSaving && isComplete(rules) && (!asksName || name.trim() !== "");
  const submit = () => {
    if (!canSave) return;
    setIsSaving(true);
    onSubmit(name, rules)
      .then((result) => {
        setRefusal(result);
        setIsSaving(false);
      })
      .catch((error: unknown) => {
        console.error("Failed to save the smart collection", error);
        setIsSaving(false);
      });
  };

  return (
    <Modal
      title={title}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={!canSave} onClick={submit}>
            {t("common.ok")}
          </Button>
        </>
      }
    >
      <div className={listStyles.form}>
        {refines !== null && (
          <p className={listStyles.hint}>
            {t("collections.refines", { name: refines })}
          </p>
        )}
        {asksName && (
          <label className={listStyles.field}>
            {t("collections.name")}
            <input
              className={listStyles.input}
              type="text"
              autoFocus
              spellCheck={false}
              value={name}
              aria-describedby={refusal === null ? undefined : errorId}
              onChange={(event) => {
                setName(event.target.value);
                setRefusal(null);
              }}
            />
          </label>
        )}
        <GroupEditor
          group={rules}
          context={context}
          onChange={(next) => {
            setRules(next);
          }}
        />
        {refusal !== null && (
          <p id={errorId} className={listStyles.error} role="alert">
            {t(`collections.refusal.${refusal}`)}
          </p>
        )}
      </div>
    </Modal>
  );
}

/** Un groupe : sa combinaison, ses règles, et de quoi en ajouter. */
function GroupEditor({
  group,
  context,
  onChange,
  onRemove,
}: {
  group: Group;
  context: Context;
  onChange: (group: Group) => void;
  /** Absent pour le groupe principal. */
  onRemove?: () => void;
}) {
  const { t } = useTranslation();
  const replace = (index: number, rule: SmartRule | null) => {
    const rules =
      rule === null
        ? group.rules.filter((_, position) => position !== index)
        : group.rules.map((current, position) =>
            position === index ? rule : current,
          );
    onChange({ ...group, rules });
  };

  return (
    <fieldset className={styles.group}>
      <legend className={styles.legend}>
        <select
          aria-label={t("smart.combine")}
          value={group.combine}
          onChange={(event) => {
            onChange({
              ...group,
              combine: event.currentTarget.value as Group["combine"],
            });
          }}
        >
          <option value="all">{t("smart.all")}</option>
          <option value="any">{t("smart.any")}</option>
        </select>
        {onRemove !== undefined && (
          <RemoveButton label={t("smart.removeGroup")} onClick={onRemove} />
        )}
      </legend>
      {group.rules.map((rule, index) =>
        rule.kind === "group" ? (
          <GroupEditor
            // Les règles n'ont pas d'identifiant : leur position en tient lieu.
            key={index}
            group={rule}
            context={context}
            onChange={(next) => {
              replace(index, next);
            }}
            onRemove={() => {
              replace(index, null);
            }}
          />
        ) : (
          <RuleEditor
            key={index}
            rule={rule}
            context={context}
            onChange={(next) => {
              replace(index, next);
            }}
            onRemove={() => {
              replace(index, null);
            }}
          />
        ),
      )}
      <div className={styles.adders}>
        <Button
          onClick={() => {
            onChange({
              ...group,
              rules: [...group.rules, defaultRule("rating")],
            });
          }}
        >
          {t("smart.addRule")}
        </Button>
        <Button
          onClick={() => {
            onChange({
              ...group,
              rules: [...group.rules, defaultRule("group")],
            });
          }}
        >
          {t("smart.addGroup")}
        </Button>
      </div>
    </fieldset>
  );
}

function RemoveButton({
  label,
  onClick,
}: {
  label: string;
  onClick: () => void;
}) {
  return (
    <button
      type="button"
      className={styles.remove}
      aria-label={label}
      title={label}
      onClick={onClick}
    >
      −
    </button>
  );
}

/** Une règle : son critère, puis ses réglages. */
function RuleEditor({
  rule,
  context,
  onChange,
  onRemove,
}: {
  rule: Exclude<SmartRule, Group>;
  context: Context;
  onChange: (rule: SmartRule) => void;
  onRemove: () => void;
}) {
  const { t } = useTranslation();
  return (
    <div className={styles.rule} role="group" aria-label={t("smart.rule")}>
      <select
        aria-label={t("smart.criterion")}
        value={rule.kind}
        onChange={(event) => {
          onChange(defaultRule(event.currentTarget.value as SmartRule["kind"]));
        }}
      >
        {SMART_CRITERIA.map((kind) => (
          <option key={kind} value={kind}>
            {t(`smart.criteria.${kind}`)}
          </option>
        ))}
      </select>
      {/* Un autre critère repart de ses propres réglages. */}
      <RuleSettings
        key={rule.kind}
        rule={rule}
        context={context}
        onChange={onChange}
      />
      <RemoveButton label={t("smart.removeRule")} onClick={onRemove} />
    </div>
  );
}

/** Réglages propres au critère d'une règle. */
function RuleSettings({
  rule,
  context,
  onChange,
}: {
  rule: Exclude<SmartRule, Group>;
  context: Context;
  onChange: (rule: SmartRule) => void;
}) {
  const { t } = useTranslation();
  const [keywordText, setKeywordText] = useState(() =>
    rule.kind === "keyword" && rule.keywordId > 0
      ? context.keywordPath(rule.keywordId)
      : "",
  );
  const select = <T extends string>(
    label: string,
    value: T,
    options: readonly T[],
    text: (option: T) => string,
    change: (value: T) => void,
  ) => (
    <select
      aria-label={label}
      value={value}
      onChange={(event) => {
        change(event.currentTarget.value as T);
      }}
    >
      {options.map((option) => (
        <option key={option} value={option}>
          {text(option)}
        </option>
      ))}
    </select>
  );
  const textInput = (
    label: string,
    value: string,
    change: (text: string) => void,
  ) => (
    <input
      type="text"
      aria-label={label}
      value={value}
      onChange={(event) => {
        change(event.currentTarget.value);
      }}
    />
  );
  const checkbox = (
    label: string,
    checked: boolean,
    change: (value: boolean) => void,
  ) => (
    <label className={styles.check}>
      <input
        type="checkbox"
        checked={checked}
        onChange={(event) => {
          change(event.currentTarget.checked);
        }}
      />
      {label}
    </label>
  );

  switch (rule.kind) {
    case "rating":
      return (
        <>
          {select(
            t("smart.comparison"),
            rule.comparison,
            ["atLeast", "equal", "atMost"] as const,
            (option) => t(`filters.rating.${option}`),
            (comparison) => {
              onChange({ ...rule, comparison });
            },
          )}
          {select(
            t("smart.stars"),
            String(rule.stars),
            ["0", "1", "2", "3", "4", "5"] as const,
            (option) => t("triage.stars", { count: Number(option) }),
            (stars) => {
              onChange({ ...rule, stars: Number(stars) });
            },
          )}
        </>
      );
    case "flag":
      return select(
        t("smart.criteria.flag"),
        rule.flag,
        ["picked", "unflagged", "rejected"] as const,
        (option) => t(`filters.flag.${option}`),
        (flag) => {
          onChange({ ...rule, flag });
        },
      );
    case "color":
      return select(
        t("smart.criteria.color"),
        rule.color,
        ["red", "yellow", "green", "blue", "purple", "none"] as const,
        (option) =>
          option === "none"
            ? t("filters.color.none")
            : t(`filters.metadata.values.${option}`),
        (color) => {
          onChange({ ...rule, color });
        },
      );
    case "keyword":
      return (
        <>
          <span className={styles.keyword}>
            <KeywordCombobox
              value={keywordText}
              multiple={false}
              label={t("smart.criteria.keyword")}
              placeholder={t("smart.keywordPlaceholder")}
              onChange={(text, picked) => {
                setKeywordText(text);
                onChange({ ...rule, keywordId: picked?.id ?? 0 });
              }}
            />
          </span>
          {checkbox(
            t("smart.includeChildren"),
            rule.includeChildren,
            (includeChildren) => {
              onChange({ ...rule, includeChildren });
            },
          )}
        </>
      );
    case "captureDate":
    case "importDate":
      return (
        <>
          {select(
            t("smart.comparison"),
            rule.comparison,
            ["before", "on", "after"] as const,
            (option) => t(`smart.date.${option}`),
            (comparison) => {
              onChange({ ...rule, comparison });
            },
          )}
          <input
            type="date"
            aria-label={t("smart.day")}
            value={rule.date}
            onChange={(event) => {
              onChange({ ...rule, date: event.currentTarget.value });
            }}
          />
        </>
      );
    case "camera":
    case "lens":
      return textInput(t("smart.contains"), rule.text, (text) => {
        onChange({ ...rule, text });
      });
    case "fileKind":
      return select(
        t("smart.criteria.fileKind"),
        rule.fileKind,
        ["photo", "video", "raw"] as const,
        (option) => t(`filters.kind.${option}`),
        (fileKind) => {
          onChange({ ...rule, fileKind });
        },
      );
    case "folder":
      return (
        <>
          <select
            aria-label={t("smart.criteria.folder")}
            value={rule.folderId}
            onChange={(event) => {
              onChange({
                ...rule,
                folderId: Number(event.currentTarget.value),
              });
            }}
          >
            <option value={0} disabled>
              {t("smart.chooseFolder")}
            </option>
            {context.folders.map((folder) => (
              <option key={folder.id} value={folder.id}>
                {folder.path}
              </option>
            ))}
          </select>
          {checkbox(
            t("smart.includeSubfolders"),
            rule.includeSubfolders,
            (includeSubfolders) => {
              onChange({ ...rule, includeSubfolders });
            },
          )}
        </>
      );
    case "text":
      return (
        <>
          {select(
            t("smart.field"),
            rule.field,
            ["fileName", "title", "caption", "place"] as const,
            (option) => t(`filters.text.fields.${option}`),
            (field) => {
              onChange({ ...rule, field });
            },
          )}
          {select(
            t("filters.text.rule"),
            rule.rule,
            [
              "contains",
              "containsAll",
              "containsWords",
              "doesNotContain",
              "startsWith",
              "endsWith",
            ] as const,
            (option) => t(`filters.text.rules.${option}`),
            (textRule) => {
              onChange({ ...rule, rule: textRule });
            },
          )}
          {textInput(t("filters.text.label"), rule.text, (text) => {
            onChange({ ...rule, text });
          })}
        </>
      );
    case "gps":
      return select(
        t("smart.criteria.gps"),
        rule.present ? "present" : "absent",
        ["present", "absent"] as const,
        (option) => t(`smart.gps.${option}`),
        (value) => {
          onChange({ ...rule, present: value === "present" });
        },
      );
    case "stack":
      return select(
        t("smart.criteria.stack"),
        rule.stacked ? "stacked" : "unstacked",
        ["stacked", "unstacked"] as const,
        (option) => t(`smart.stack.${option}`),
        (value) => {
          onChange({ ...rule, stacked: value === "stacked" });
        },
      );
  }
}
