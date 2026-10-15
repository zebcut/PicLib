import type { TFunction } from "i18next";
import type { MenuOptions } from "@tauri-apps/api/menu";

import type { KeywordListItemDto } from "../../services/keywordList";

/** Actions du menu contextuel de la liste des mots-clés. */
export interface KeywordMenuActions {
  /** Crée un mot-clé sous `parent` (`null` : à la racine). */
  create: (parent: KeywordListItemDto | null) => void;
  rename: (keyword: KeywordListItemDto) => void;
  move: (keyword: KeywordListItemDto) => void;
  merge: (keyword: KeywordListItemDto) => void;
  remove: (keyword: KeywordListItemDto) => void;
  setExport: (keyword: KeywordListItemDto, include: boolean) => void;
  showPhotos: (keyword: KeywordListItemDto) => void;
  deleteUnused: () => void;
  /** Importe ou exporte la liste (absents : éléments grisés). */
  importList: (() => void) | undefined;
  exportList: (() => void) | undefined;
}

/**
 * Éléments du menu contextuel (clic droit) d'un mot-clé de la liste, ou de
 * la liste elle-même (`keyword` à `null`, avec l'import et l'export de la
 * liste), comme dans Lightroom.
 */
export function keywordMenuItems(
  t: TFunction,
  keyword: KeywordListItemDto | null,
  actions: KeywordMenuActions,
): NonNullable<MenuOptions["items"]> {
  const separator = { item: "Separator" } as const;
  const deleteUnused = {
    id: "keyword.deleteUnused",
    text: t("keywordList.menu.deleteUnused"),
    action: actions.deleteUnused,
  };
  if (keyword === null) {
    return [
      {
        id: "keyword.createRoot",
        text: t("keywordList.menu.createRoot"),
        action: () => {
          actions.create(null);
        },
      },
      separator,
      deleteUnused,
      separator,
      {
        id: "keyword.importList",
        text: t("keywordList.menu.importKeywords"),
        enabled: actions.importList !== undefined,
        action: actions.importList,
      },
      {
        id: "keyword.exportList",
        text: t("keywordList.menu.exportKeywords"),
        enabled: actions.exportList !== undefined,
        action: actions.exportList,
      },
    ];
  }
  return [
    {
      id: "keyword.createInside",
      text: t("keywordList.menu.createInside", { name: keyword.name }),
      action: () => {
        actions.create(keyword);
      },
    },
    {
      id: "keyword.rename",
      text: t("keywordList.menu.rename"),
      action: () => {
        actions.rename(keyword);
      },
    },
    {
      id: "keyword.move",
      text: t("keywordList.menu.move"),
      action: () => {
        actions.move(keyword);
      },
    },
    {
      id: "keyword.merge",
      text: t("keywordList.menu.merge"),
      action: () => {
        actions.merge(keyword);
      },
    },
    {
      id: "keyword.delete",
      text: t("keywordList.menu.delete"),
      action: () => {
        actions.remove(keyword);
      },
    },
    separator,
    {
      id: "keyword.export",
      text: t("keywordList.menu.includeOnExport"),
      checked: keyword.includeOnExport,
      action: () => {
        actions.setExport(keyword, !keyword.includeOnExport);
      },
    },
    {
      id: "keyword.showPhotos",
      text: t("keywordList.menu.showPhotos"),
      enabled: keyword.photoCount > 0,
      action: () => {
        actions.showPhotos(keyword);
      },
    },
    separator,
    deleteUnused,
  ];
}
