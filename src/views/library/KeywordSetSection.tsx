import type { TFunction } from "i18next";
import type { MenuOptions } from "@tauri-apps/api/menu";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import type { KeywordListItemDto } from "../../services/keywordList";
import type { KeywordPresenceDto } from "../../services/keywords";
import type {
  KeywordSetDto,
  KeywordSetOperation,
} from "../../services/keywordSets";
import type { KeywordSets } from "../../state/useKeywordSets";
import {
  DEFAULT_BINDINGS,
  isApplePlatform,
  KEYWORD_SET_SLOTS,
  keywordSetShortcut,
  shortcutLabel,
} from "../../shortcuts/registry";
import { popUpContextMenu } from "../../menu/contextMenu";
import { itemPath } from "../../utils/tree";
import { KeywordSetDialog } from "./KeywordSetDialogs";
import type { KeywordSetDialogState } from "./KeywordSetDialogs";
import shared from "./KeywordSection.module.css";
import styles from "./KeywordSetSection.module.css";

/**
 * Section « Jeu de mots-clés » du panneau Mots-clés : le jeu
 * actif et ses neuf mots-clés. Un clic (ou Option+1 à Option+9) pose le
 * mot-clé sur la sélection, ou l'en retire s'il est sur toutes les photos.
 * Une case vide ouvre la fenêtre du jeu ; le bouton « … » crée, modifie,
 * renomme ou supprime un jeu.
 *
 * @param presence présence des mots-clés sur la sélection.
 * @param canApply faux sans photo sélectionnée.
 * @param onToggle pose ou retire un mot-clé du jeu.
 */
export function KeywordSetSection({
  sets,
  keywords,
  presence,
  canApply,
  onToggle,
}: {
  sets: KeywordSets;
  keywords: readonly KeywordListItemDto[];
  presence: ReadonlyMap<number, KeywordPresenceDto>;
  canApply: boolean;
  onToggle: (keywordId: number) => void;
}) {
  const { t } = useTranslation();
  const labelId = useId();
  const [dialog, setDialog] = useState<KeywordSetDialogState | null>(null);
  const [failed, setFailed] = useState(false);
  const byId = useMemo(
    () => new Map(keywords.map((keyword) => [keyword.id, keyword])),
    [keywords],
  );

  const list = sets.state.status === "ready" ? sets.state.sets : [];
  const active = list.find((set) => set.active) ?? null;

  const run = (operation: KeywordSetOperation) =>
    sets.change(operation).then((refusal) => {
      if (refusal === null) setDialog(null);
      return refusal;
    });
  const openMenu = () => {
    popUpContextMenu(setMenuItems(t, active, setDialog)).catch(
      (error: unknown) => {
        console.error("Failed to open the keyword set menu", error);
      },
    );
  };

  return (
    <div className={shared.section}>
      <div className={shared.header}>
        <span id={labelId} className={shared.label}>
          {t("keywordSets.title")}
        </span>
        {active !== null && (
          <select
            className={styles.choice}
            aria-labelledby={labelId}
            value={active.id}
            onChange={(event) => {
              run({ kind: "activate", setId: Number(event.target.value) })
                .then(() => {
                  setFailed(false);
                })
                .catch((error: unknown) => {
                  console.error("Failed to choose the keyword set", error);
                  setFailed(true);
                });
            }}
          >
            {list.map((set) => (
              <option key={set.id} value={set.id}>
                {set.name}
              </option>
            ))}
          </select>
        )}
        <button
          type="button"
          className={styles.menuButton}
          aria-label={t("keywordSets.actions")}
          title={t("keywordSets.actions")}
          onClick={openMenu}
        >
          …
        </button>
      </div>
      {sets.state.status === "failed" && (
        <p className={styles.error} role="alert">
          {t("keywordSets.loadFailed")}
        </p>
      )}
      {sets.state.status === "ready" && active === null && (
        <div className={styles.empty}>
          <p className={shared.hint}>{t("keywordSets.empty")}</p>
          <Button
            onClick={() => {
              setDialog({ kind: "create" });
            }}
          >
            {t("keywordSets.create")}
          </Button>
        </div>
      )}
      {active !== null && (
        <SlotGrid
          set={active}
          byId={byId}
          presence={presence}
          canApply={canApply}
          onToggle={onToggle}
          onEdit={(slot) => {
            setDialog({ kind: "edit", set: active, slot });
          }}
        />
      )}
      {failed && (
        <p className={styles.error} role="alert">
          {t("keywordSets.failed")}
        </p>
      )}
      {dialog !== null && (
        <KeywordSetDialog
          dialog={dialog}
          keywords={keywords}
          onRun={run}
          onClose={() => {
            setDialog(null);
          }}
        />
      )}
    </div>
  );
}

/** Les neuf boutons du jeu actif, en trois rangées de trois. */
function SlotGrid({
  set,
  byId,
  presence,
  canApply,
  onToggle,
  onEdit,
}: {
  set: KeywordSetDto;
  byId: ReadonlyMap<number, KeywordListItemDto>;
  presence: ReadonlyMap<number, KeywordPresenceDto>;
  canApply: boolean;
  onToggle: (keywordId: number) => void;
  onEdit: (slot: number) => void;
}) {
  const { t } = useTranslation();
  const isApple = isApplePlatform();
  return (
    <div className={shared.grid} role="group" aria-label={set.name}>
      {KEYWORD_SET_SLOTS.map((slot, index) => {
        const binding = DEFAULT_BINDINGS[keywordSetShortcut(slot)];
        const shortcut =
          binding === null ? "" : shortcutLabel(binding, isApple);
        const keywordId = set.slots[index] ?? null;
        const keyword = keywordId === null ? undefined : byId.get(keywordId);
        if (keyword === undefined) {
          return (
            <button
              key={slot}
              type="button"
              className={[shared.slot, shared.emptySlot].join(" ")}
              aria-label={t("keywordSets.fillSlot", { shortcut })}
              title={t("keywordSets.fillSlot", { shortcut })}
              onClick={() => {
                onEdit(slot);
              }}
            >
              <span className={shared.key}>{shortcut}</span>
            </button>
          );
        }
        const state = presence.get(keyword.id) ?? "absent";
        return (
          <button
            key={slot}
            type="button"
            className={[
              shared.slot,
              state === "all" ? shared.on : undefined,
              state === "partial" ? shared.partial : undefined,
            ]
              .join(" ")
              .trim()}
            aria-pressed={
              state === "all" ? true : state === "partial" ? "mixed" : false
            }
            disabled={!canApply}
            title={`${itemPath(byId, keyword.id)} (${shortcut})`}
            onClick={() => {
              onToggle(keyword.id);
            }}
          >
            <span className={shared.slotName}>{keyword.name}</span>
            <span className={shared.key} aria-hidden>
              {shortcut}
            </span>
          </button>
        );
      })}
    </div>
  );
}

/** Menu « … » de la section : créer, modifier, renommer, supprimer. */
function setMenuItems(
  t: TFunction,
  active: KeywordSetDto | null,
  open: (dialog: KeywordSetDialogState) => void,
): NonNullable<MenuOptions["items"]> {
  return [
    {
      id: "keywordSet.create",
      text: t("keywordSets.menu.create"),
      action: () => {
        open({ kind: "create" });
      },
    },
    { item: "Separator" },
    {
      id: "keywordSet.edit",
      text: t("keywordSets.menu.edit"),
      enabled: active !== null,
      action: () => {
        if (active !== null) open({ kind: "edit", set: active, slot: 1 });
      },
    },
    {
      id: "keywordSet.rename",
      text: t("keywordSets.menu.rename"),
      enabled: active !== null,
      action: () => {
        if (active !== null) open({ kind: "rename", set: active });
      },
    },
    {
      id: "keywordSet.delete",
      text: t("keywordSets.menu.delete"),
      enabled: active !== null,
      action: () => {
        if (active !== null) open({ kind: "delete", set: active });
      },
    },
  ];
}
