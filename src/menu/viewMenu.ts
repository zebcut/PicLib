import type { TFunction } from "i18next";
import { Submenu } from "@tauri-apps/api/menu";

import type { ShortcutTrigger } from "../shortcuts/ShortcutProvider";
import { menuAccelerator } from "../shortcuts/registry";
import type { ShortcutId } from "../shortcuts/registry";

/**
 * Menu Affichage : Grille, Loupe, mode Tri plein écran, Comparaison,
 * Ensemble et Carte ; barre de filtres et filtres (\, Cmd+L). Chaque commande
 * déclenche l'action du registre des raccourcis, comme sa touche (G, E, F,
 * C, N, \), que le menu n'affiche pas : un raccourci
 * d'une lettre déclaré dans un menu natif empêcherait de taper cette lettre
 * dans les champs de saisie.
 *
 * @param enabled faux tant qu'aucune bibliothèque n'est affichée.
 */
export function createViewMenu(
  t: TFunction,
  trigger: ShortcutTrigger,
  enabled: boolean,
): Promise<Submenu> {
  const item = (id: ShortcutId, text: string) => ({
    id,
    text,
    enabled,
    accelerator: menuAccelerator(id),
    action: () => {
      trigger(id);
    },
  });
  return Submenu.new({
    text: t("menu.view"),
    items: [
      item("view.grid", t("menu.grid")),
      item("view.loupe", t("menu.loupe")),
      item("view.cull", t("menu.cull")),
      item("view.compare", t("menu.compare")),
      item("view.survey", t("menu.survey")),
      item("view.map", t("menu.map")),
      { item: "Separator" } as const,
      item("view.toggleFilmstrip", t("menu.toggleFilmstrip")),
      item("filters.toggleBar", t("menu.toggleFilterBar")),
      item("filters.toggleEnabled", t("menu.toggleFilters")),
    ],
  });
}
