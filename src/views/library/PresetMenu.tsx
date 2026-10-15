import { useState } from "react";
import { useTranslation } from "react-i18next";

import { popUpContextMenu } from "../../menu/contextMenu";
import type {
  FilterPresetContent,
  FilterPresetDto,
  FilterPresetOperation,
  FilterPresetRefusal,
} from "../../services/filterPresets";
import type { GridMetadataColumn } from "../../services/metadataBrowser";
import {
  BUILT_IN_PRESETS,
  presetContent,
  presetKey,
} from "../../state/filterPresets";
import type { FilterBarState } from "../../state/gridFilter";
import { useFilterPresets } from "../../state/useFilterPresets";
import styles from "./FilterBar.module.css";
import { NameDialog } from "./KeywordDialogs";

/** Valeur de la liste quand l'état ne correspond à aucun préréglage. */
const CUSTOM = "custom";

/** Fenêtre de nom ouverte : nouveau préréglage, ou renommage. */
type NameRequest =
  { kind: "create" } | { kind: "rename"; preset: FilterPresetDto };

/**
 * Préréglages de la barre de filtres : la liste des préréglages
 * fournis et des siens, qui affiche celui qui correspond à l'état de la
 * barre (ou « Personnalisé ») ; un menu pour enregistrer, mettre à jour,
 * renommer ou supprimer les siens, chaque fois annulable (Cmd+Z).
 *
 * @param onApply applique le contenu d'un préréglage.
 * @param onChangePreset absent : les préréglages de l'utilisateur ne sont
 *   pas proposés.
 */
export function PresetMenu({
  state,
  columns,
  refreshKey,
  onApply,
  onChangePreset,
}: {
  state: FilterBarState;
  columns: readonly GridMetadataColumn[];
  refreshKey: number;
  onApply: (content: FilterPresetContent) => void;
  onChangePreset?: (
    operation: FilterPresetOperation,
  ) => Promise<FilterPresetRefusal | null>;
}) {
  const { t } = useTranslation();
  const presets = useFilterPresets(refreshKey);
  // Dernier préréglage à soi choisi : « Mettre à jour » s'y applique même
  // une fois les critères changés.
  const [lastPresetId, setLastPresetId] = useState<number | null>(null);
  const [nameRequest, setNameRequest] = useState<NameRequest | null>(null);

  const content = presetContent(state, columns);
  const key = presetKey(content);
  const mine = presets.find((preset) => presetKey(preset.content) === key);
  const builtIn = BUILT_IN_PRESETS.find(
    (preset) => presetKey(preset.content) === key,
  );
  const selected =
    mine !== undefined
      ? `mine-${String(mine.id)}`
      : builtIn !== undefined
        ? `builtIn-${builtIn.key}`
        : CUSTOM;
  const last = presets.find(
    (preset) => preset.id === (mine?.id ?? lastPresetId),
  );

  const choose = (value: string) => {
    const userPreset = presets.find(
      (preset) => `mine-${String(preset.id)}` === value,
    );
    const chosen =
      userPreset?.content ??
      BUILT_IN_PRESETS.find((preset) => `builtIn-${preset.key}` === value)
        ?.content;
    if (chosen === undefined) return;
    setLastPresetId(userPreset?.id ?? null);
    onApply(chosen);
  };

  const openMenu = (change: NonNullable<typeof onChangePreset>) => {
    const separator = { item: "Separator" } as const;
    const items = [
      {
        text: t("filters.presets.save"),
        action: () => {
          setNameRequest({ kind: "create" });
        },
      },
      ...(last === undefined
        ? []
        : [
            separator,
            {
              text: t("filters.presets.update", { name: last.name }),
              enabled: mine?.id !== last.id,
              action: () => {
                void change({
                  kind: "update",
                  presetId: last.id,
                  content,
                });
              },
            },
            {
              text: t("filters.presets.rename", { name: last.name }),
              action: () => {
                setNameRequest({ kind: "rename", preset: last });
              },
            },
            {
              text: t("filters.presets.delete", { name: last.name }),
              action: () => {
                void change({ kind: "delete", presetId: last.id });
              },
            },
          ]),
    ];
    popUpContextMenu(items).catch((error: unknown) => {
      console.error("Failed to open the presets menu", error);
    });
  };

  return (
    <span className={styles.presets}>
      <select
        className={styles.comparison}
        aria-label={t("filters.presets.label")}
        value={selected}
        onChange={(event) => {
          choose(event.currentTarget.value);
        }}
      >
        <optgroup label={t("filters.presets.builtInGroup")}>
          {BUILT_IN_PRESETS.map((preset) => (
            <option key={preset.key} value={`builtIn-${preset.key}`}>
              {t(
                `filters.presets.builtIn.${preset.key}` as "filters.presets.builtIn.off",
              )}
            </option>
          ))}
        </optgroup>
        {presets.length > 0 && (
          <optgroup label={t("filters.presets.mine")}>
            {presets.map((preset) => (
              <option key={preset.id} value={`mine-${String(preset.id)}`}>
                {preset.name}
              </option>
            ))}
          </optgroup>
        )}
        {selected === CUSTOM && (
          <option value={CUSTOM} disabled>
            {last === undefined
              ? t("filters.presets.custom")
              : t("filters.presets.edited", { name: last.name })}
          </option>
        )}
      </select>
      {onChangePreset !== undefined && (
        <button
          type="button"
          className={styles.tab}
          aria-label={t("filters.presets.manage")}
          title={t("filters.presets.manage")}
          onClick={() => {
            openMenu(onChangePreset);
          }}
        >
          ⋯
        </button>
      )}
      {nameRequest !== null && onChangePreset !== undefined && (
        <NameDialog<FilterPresetRefusal>
          title={
            nameRequest.kind === "create"
              ? t("filters.presets.newTitle")
              : t("filters.presets.renameTitle", {
                  name: nameRequest.preset.name,
                })
          }
          initialName={
            nameRequest.kind === "create" ? "" : nameRequest.preset.name
          }
          askExport={false}
          onSubmit={async (name) => {
            const refusal = await onChangePreset(
              nameRequest.kind === "create"
                ? { kind: "create", name, content }
                : { kind: "rename", presetId: nameRequest.preset.id, name },
            );
            if (refusal === null) setNameRequest(null);
            return refusal;
          }}
          refusalText={(refusal) => t(`filters.presets.refusal.${refusal}`)}
          onClose={() => {
            setNameRequest(null);
          }}
        />
      )}
    </span>
  );
}
