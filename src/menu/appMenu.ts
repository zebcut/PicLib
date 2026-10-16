import type { TFunction } from "i18next";
import { Menu, Submenu } from "@tauri-apps/api/menu";

import type { ShortcutTrigger } from "../shortcuts/ShortcutProvider";
import { menuAccelerator } from "../shortcuts/registry";
import { createPhotoMenu } from "./photoMenu";
import type { PhotoMenuState } from "./photoMenu";
import { createViewMenu } from "./viewMenu";

/** Actions déclenchées depuis la barre de menus. */
export interface AppMenuActions {
  openCatalog: () => void;
  createCatalog: () => void;
  openPreferences: () => void;
  /** Absent tant que le catalogue n'est pas ouvert : l'élément est grisé. */
  importPhotos?: () => void;
  /** Déclenche une action du registre des raccourcis (Tout sélectionner…). */
  trigger: ShortcutTrigger;
  /**
   * Libellés d'Annuler et Rétablir (« Annuler « Note : 3 étoiles » ») ;
   * `null` : rien à annuler ou rétablir, l'élément est grisé.
   */
  undoLabel: string | null;
  redoLabel: string | null;
  /** Menu Photo. */
  photo: PhotoMenuState;
  /**
   * Import et export de la liste des mots-clés (menu Métadonnées) ; absents
   * tant que le catalogue n'est pas ouvert : les éléments sont grisés.
   */
  importKeywords?: () => void;
  exportKeywords?: () => void;
}

/** Installations du menu en attente, faites l'une après l'autre. */
let installQueue: Promise<void> = Promise.resolve();

/** Numéro de la dernière installation demandée. */
let latestInstall = 0;

/**
 * Installe la barre de menus native de l'application, traduite dans la langue
 * de l'interface.
 *
 * Les éléments « prédéfinis » (Copier, Masquer, Quitter...) sont gérés par le
 * système, avec leurs raccourcis standard. Ceux des commandes de PicLib
 * viennent du registre central des raccourcis.
 *
 * Le menu est réinstallé à chaque changement d'état. Ses créations passent
 * par Tauri et peuvent finir dans le désordre : un menu demandé avant
 * l'ouverture du catalogue, tout grisé, remplaçait alors le menu actif. Les
 * installations se font donc l'une après l'autre, et celles dépassées par une
 * demande plus récente sont sautées.
 *
 * @throws si Tauri refuse de créer le menu.
 */
export function installAppMenu(
  t: TFunction,
  actions: AppMenuActions,
): Promise<void> {
  latestInstall += 1;
  const install = latestInstall;
  const installed = installQueue.then(() =>
    install === latestInstall ? buildAppMenu(t, actions) : undefined,
  );
  // Un échec ne bloque pas les installations suivantes.
  installQueue = installed.catch(() => undefined);
  return installed;
}

async function buildAppMenu(
  t: TFunction,
  actions: AppMenuActions,
): Promise<void> {
  const separator = { item: "Separator" } as const;

  // Sur macOS, le premier sous-menu est le menu de l'application.
  const appSubmenu = await Submenu.new({
    text: t("menu.app"),
    items: [
      {
        // Mention due à la base des lieux intégrée (licence CC BY 4.0).
        item: { About: { credits: t("menu.aboutCredits") } },
        text: t("menu.about"),
      },
      separator,
      {
        id: "preferences",
        text: t("menu.preferences"),
        accelerator: menuAccelerator("app.preferences"),
        action: actions.openPreferences,
      },
      separator,
      { item: "Services", text: t("menu.services") },
      separator,
      { item: "Hide", text: t("menu.hide") },
      { item: "HideOthers", text: t("menu.hideOthers") },
      { item: "ShowAll", text: t("menu.showAll") },
      separator,
      { item: "Quit", text: t("menu.quit") },
    ],
  });
  const fileSubmenu = await Submenu.new({
    text: t("menu.file"),
    items: [
      {
        id: "open-catalog",
        text: t("menu.openCatalog"),
        action: actions.openCatalog,
      },
      {
        id: "new-catalog",
        text: t("menu.newCatalog"),
        action: actions.createCatalog,
      },
      {
        id: "catalog.cleanUp",
        text: t("menu.cleanUpCatalog"),
        enabled: actions.photo.enabled,
        action: () => {
          actions.trigger("catalog.cleanUp");
        },
      },
      separator,
      {
        id: "import-photos",
        text: t("menu.importPhotos"),
        enabled: actions.importPhotos !== undefined,
        action: actions.importPhotos,
      },
    ],
  });
  // Couper, Copier, Coller : indispensables sur macOS pour les champs de
  // saisie. Annuler et Rétablir agissent sur le catalogue, ou sur le texte
  // d'un champ quand il a le focus.
  const editSubmenu = await Submenu.new({
    text: t("menu.edit"),
    items: [
      {
        id: "undo",
        text: actions.undoLabel ?? t("menu.undo"),
        enabled: actions.undoLabel !== null,
        accelerator: menuAccelerator("edit.undo"),
        action: () => {
          if (!editField("undo")) actions.trigger("edit.undo");
        },
      },
      {
        id: "redo",
        text: actions.redoLabel ?? t("menu.redo"),
        enabled: actions.redoLabel !== null,
        accelerator: menuAccelerator("edit.redo"),
        action: () => {
          if (!editField("redo")) actions.trigger("edit.redo");
        },
      },
      separator,
      { item: "Cut", text: t("menu.cut") },
      { item: "Copy", text: t("menu.copy") },
      { item: "Paste", text: t("menu.paste") },
      separator,
      {
        id: "select-all",
        text: t("menu.selectAll"),
        accelerator: menuAccelerator("selection.all"),
        action: () => {
          // Dans un champ de saisie, Cmd+A sélectionne son texte.
          if (!selectFieldText()) actions.trigger("selection.all");
        },
      },
      {
        id: "select-none",
        text: t("menu.selectNone"),
        accelerator: menuAccelerator("selection.none"),
        action: () => {
          actions.trigger("selection.none");
        },
      },
    ],
  });
  // Les vues n'existent qu'avec une bibliothèque affichée, comme le menu Photo.
  const viewSubmenu = await createViewMenu(
    t,
    actions.trigger,
    actions.photo.enabled,
  );
  const librarySubmenu = await Submenu.new({
    text: t("menu.library"),
    items: [
      {
        id: "library.findMissing",
        text: t("menu.findMissing"),
        enabled: actions.photo.enabled,
        action: () => {
          actions.trigger("library.findMissing");
        },
      },
      {
        id: "library.operationsLog",
        text: t("menu.operationsLog"),
        enabled: actions.photo.enabled,
        action: () => {
          actions.trigger("library.operationsLog");
        },
      },
    ],
  });
  const photoSubmenu = await createPhotoMenu(t, actions.trigger, actions.photo);
  const metadataSubmenu = await Submenu.new({
    text: t("menu.metadata"),
    items: [
      {
        id: "metadata.readFromFiles",
        text: t("menu.readMetadata"),
        enabled: actions.photo.enabled,
        accelerator: menuAccelerator("metadata.readFromFiles"),
        action: () => {
          actions.trigger("metadata.readFromFiles");
        },
      },
      {
        id: "metadata.saveToFiles",
        text: t("menu.saveToFiles"),
        enabled: actions.photo.enabled,
        accelerator: menuAccelerator("metadata.saveToFiles"),
        action: () => {
          actions.trigger("metadata.saveToFiles");
        },
      },
      {
        id: "metadata.removeLocation",
        text: t("menu.removeLocation"),
        enabled: actions.photo.enabled,
        accelerator: menuAccelerator("metadata.removeLocation"),
        action: () => {
          actions.trigger("metadata.removeLocation");
        },
      },
      separator,
      {
        id: "import-keywords",
        text: t("menu.importKeywords"),
        enabled: actions.importKeywords !== undefined,
        action: actions.importKeywords,
      },
      {
        id: "export-keywords",
        text: t("menu.exportKeywords"),
        enabled: actions.exportKeywords !== undefined,
        action: actions.exportKeywords,
      },
    ],
  });
  const windowSubmenu = await Submenu.new({
    text: t("menu.window"),
    items: [
      { item: "Minimize", text: t("menu.minimize") },
      { item: "Maximize", text: t("menu.zoom") },
    ],
  });

  const menu = await Menu.new({
    items: [
      appSubmenu,
      fileSubmenu,
      editSubmenu,
      viewSubmenu,
      librarySubmenu,
      photoSubmenu,
      metadataSubmenu,
      windowSubmenu,
    ],
  });
  await menu.setAsAppMenu();
}

/** Champ de saisie de texte qui a le focus, s'il y en a un. */
function focusedTextField(): HTMLInputElement | HTMLTextAreaElement | null {
  const field = document.activeElement;
  return field instanceof HTMLTextAreaElement ||
    (field instanceof HTMLInputElement && field.type !== "range")
    ? field
    : null;
}

/**
 * Sélectionne le texte du champ de saisie actif ; renvoie `false` si aucun
 * champ n'a le focus.
 */
function selectFieldText(): boolean {
  const field = focusedTextField();
  field?.select();
  return field !== null;
}

/**
 * Annule ou rétablit la dernière frappe dans le champ de saisie actif ;
 * renvoie `false` si aucun champ n'a le focus.
 */
function editField(command: "undo" | "redo"): boolean {
  if (focusedTextField() === null) return false;
  // Seul moyen d'atteindre l'historique de frappe du navigateur ; déprécié
  // mais toujours pris en charge par les vues web de macOS et Windows.
  // eslint-disable-next-line @typescript-eslint/no-deprecated
  document.execCommand(command);
  return true;
}
