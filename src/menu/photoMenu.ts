import type { TFunction } from "i18next";
import { Submenu } from "@tauri-apps/api/menu";
import type { SubmenuOptions } from "@tauri-apps/api/menu";

import type { ShortcutTrigger } from "../shortcuts/ShortcutProvider";
import { menuAccelerator } from "../shortcuts/registry";
import type { ShortcutId } from "../shortcuts/registry";

/** État du menu Photo. */
export interface PhotoMenuState {
  /** Faux tant qu'aucune bibliothèque n'est affichée : commandes grisées. */
  enabled: boolean;
  /** Avance automatique cochée. */
  autoAdvance: boolean;
  /** Coche ou décoche l'avance automatique. */
  toggleAutoAdvance: () => void;
  /** Avance automatique du mode Tri plein écran cochée. */
  cullAutoAdvance: boolean;
  /** Coche ou décoche l'avance automatique du mode Tri. */
  toggleCullAutoAdvance: () => void;
  /** Éditeurs externes proposés. */
  editors: readonly { id: string; name: string }[];
  /** Nom de l'éditeur de Cmd+E ; `null` : aucun. */
  defaultEditorName: string | null;
  /** Ouvre la sélection dans un éditeur. */
  editIn: (editorId: string) => void;
}

/**
 * Menu Photo : marqueurs, notes et étiquettes de couleur des photos
 * sélectionnées (dont le violet, sans raccourci), rotation, et l'avance
 * automatique (Grille et Loupe d'une part, mode Tri de l'autre).
 * Chaque commande déclenche l'action du registre des raccourcis, comme la
 * touche correspondante.
 */
export function createPhotoMenu(
  t: TFunction,
  trigger: ShortcutTrigger,
  state: PhotoMenuState,
): Promise<Submenu> {
  const item = (id: ShortcutId, text: string) => ({
    id,
    text,
    enabled: state.enabled,
    accelerator: menuAccelerator(id),
    action: () => {
      trigger(id);
    },
  });
  const separator = { item: "Separator" } as const;
  const ratingItems = ([1, 2, 3, 4, 5] as const).map((stars) =>
    item(
      `rating.${String(stars)}` as ShortcutId,
      t("triage.stars", { count: stars }),
    ),
  );
  const options: SubmenuOptions = {
    text: t("menu.photo"),
    items: [
      item("flag.picked", t("menu.setPicked")),
      item("flag.rejected", t("menu.setRejected")),
      item("flag.none", t("menu.removeFlag")),
      separator,
      {
        text: t("menu.rating"),
        items: [item("rating.0", t("menu.noRating")), ...ratingItems],
      },
      {
        text: t("menu.colorLabel"),
        items: [
          item("color.red", t("menu.red")),
          item("color.yellow", t("menu.yellow")),
          item("color.green", t("menu.green")),
          item("color.blue", t("menu.blue")),
          item("color.purple", t("menu.purple")),
          separator,
          item("color.none", t("menu.noColor")),
        ],
      },
      separator,
      item("photo.rotateLeft", t("menu.rotateLeft")),
      item("photo.rotateRight", t("menu.rotateRight")),
      separator,
      {
        ...item(
          "photo.editExternal",
          state.defaultEditorName === null
            ? t("menu.editInNoEditor")
            : t("menu.editIn", { editor: state.defaultEditorName }),
        ),
        enabled: state.enabled && state.defaultEditorName !== null,
      },
      {
        text: t("menu.editInSubmenu"),
        enabled: state.enabled && state.editors.length > 0,
        items: state.editors.map((editor) => ({
          id: `edit-in-${editor.id}`,
          text: editor.name,
          enabled: state.enabled,
          action: () => {
            state.editIn(editor.id);
          },
        })),
      },
      item("photo.revealInFinder", t("menu.revealInFinder")),
      separator,
      item("photo.rename", t("menu.renamePhotos")),
      item("photo.delete", t("menu.deletePhotos")),
      item("photo.deleteRejected", t("menu.deleteRejected")),
      item("photo.removeFromCollection", t("menu.removeFromCollection")),
      separator,
      item("collection.toggleTarget", t("menu.toggleTarget")),
      item("collection.showQuick", t("menu.showQuick")),
      item("collection.clearQuick", t("menu.clearQuick")),
      item("collection.saveQuick", t("menu.saveQuick")),
      separator,
      item("stack.group", t("menu.stackGroup")),
      item("stack.ungroup", t("menu.stackUngroup")),
      item("stack.toggle", t("menu.stackToggle")),
      item("stack.autoPairs", t("menu.stackPairs")),
      item("stack.autoBursts", t("menu.stackBursts")),
      separator,
      {
        id: "auto-advance",
        text: t("menu.autoAdvance"),
        checked: state.autoAdvance,
        action: state.toggleAutoAdvance,
      },
      {
        id: "cull-auto-advance",
        text: t("menu.cullAutoAdvance"),
        checked: state.cullAutoAdvance,
        action: state.toggleCullAutoAdvance,
      },
    ],
  };
  return Submenu.new(options);
}
