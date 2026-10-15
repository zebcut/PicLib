/**
 * Registre central des raccourcis clavier : chaque action a un identifiant et
 * une touche par défaut (celle de Lightroom quand elle existe). Les touches
 * ne sont déclarées qu'ici ; une future personnalisation remplacera cette
 * table sans toucher aux composants.
 */

/** Actions déclenchables au clavier. */
export type ShortcutId =
  | "selection.previous"
  | "selection.next"
  | "selection.above"
  | "selection.below"
  | "selection.extendPrevious"
  | "selection.extendNext"
  | "selection.extendAbove"
  | "selection.extendBelow"
  | "selection.all"
  | "selection.none"
  | "view.grid"
  | "view.loupe"
  | "view.toggleLoupe"
  | "view.cull"
  | "view.compare"
  | "view.survey"
  | "view.map"
  | "view.toggleFilmstrip"
  | "compare.choose"
  | "compare.swap"
  | "view.back"
  | "filters.toggleBar"
  | "filters.toggleEnabled"
  | "loupe.toggleZoom"
  | "flag.picked"
  | "flag.rejected"
  | "flag.none"
  | "rating.0"
  | "rating.1"
  | "rating.2"
  | "rating.3"
  | "rating.4"
  | "rating.5"
  | "color.red"
  | "color.yellow"
  | "color.green"
  | "color.blue"
  | "color.purple"
  | "color.none"
  | "photo.deleteRejected"
  | "photo.delete"
  | "photo.rename"
  | "photo.editExternal"
  | "photo.revealInFinder"
  | "photo.removeFromCollection"
  | "collection.toggleTarget"
  | "collection.showQuick"
  | "collection.clearQuick"
  | "collection.saveQuick"
  | "stack.group"
  | "stack.ungroup"
  | "stack.toggle"
  | "stack.autoPairs"
  | "stack.autoBursts"
  | "photo.rotateLeft"
  | "photo.rotateRight"
  | "metadata.readFromFiles"
  | "metadata.saveToFiles"
  | "metadata.removeLocation"
  | "library.findMissing"
  | "library.operationsLog"
  | "keywordSet.1"
  | "keywordSet.2"
  | "keywordSet.3"
  | "keywordSet.4"
  | "keywordSet.5"
  | "keywordSet.6"
  | "keywordSet.7"
  | "keywordSet.8"
  | "keywordSet.9"
  | "edit.undo"
  | "edit.redo"
  | "app.preferences";

/**
 * Combinaison de touches. `primary` est la touche de commande principale :
 * Cmd sur macOS, Ctrl sous Windows (de même, Option ↔ Alt pour `alt`).
 */
export interface KeyBinding {
  /**
   * Valeur de `KeyboardEvent.key` : « ArrowLeft », « g », « Escape »… et un
   * simple espace `" "` pour la barre d'espace.
   */
  key: string;
  /**
   * Touche physique (`KeyboardEvent.code`, par exemple « Digit3 ») acceptée
   * en plus de `key` : sur un clavier AZERTY, la touche « 3 » donne « " »
   * sans Maj, et doit pourtant noter trois étoiles, comme dans Lightroom.
   */
  code?: string;
  primary?: boolean;
  shift?: boolean;
  alt?: boolean;
}

/** Chiffre de la rangée du haut, reconnu quelle que soit la disposition. */
function digit(value: number): KeyBinding {
  return { key: String(value), code: `Digit${String(value)}` };
}

/**
 * Option + chiffre : sur un Mac, la touche donne alors un autre caractère
 * (« ¡ », « “ »…), seule la touche physique la reconnaît.
 */
function altDigit(value: number): KeyBinding {
  return { ...digit(value), alt: true };
}

/** Numéros des cases d'un jeu de mots-clés. */
export const KEYWORD_SET_SLOTS = [1, 2, 3, 4, 5, 6, 7, 8, 9] as const;

/** Action qui pose ou retire le mot-clé de la case `slot` du jeu actif. */
export function keywordSetShortcut(
  slot: (typeof KEYWORD_SET_SLOTS)[number],
): ShortcutId {
  return `keywordSet.${String(slot)}` as ShortcutId;
}

/**
 * Touches par défaut(section 16). `null` : action sans raccourci,
 * accessible par un menu seulement (étiquette violette, par exemple).
 */
export const DEFAULT_BINDINGS: Readonly<Record<ShortcutId, KeyBinding | null>> =
  {
    "selection.previous": { key: "ArrowLeft" },
    "selection.next": { key: "ArrowRight" },
    "selection.above": { key: "ArrowUp" },
    "selection.below": { key: "ArrowDown" },
    "selection.extendPrevious": { key: "ArrowLeft", shift: true },
    "selection.extendNext": { key: "ArrowRight", shift: true },
    "selection.extendAbove": { key: "ArrowUp", shift: true },
    "selection.extendBelow": { key: "ArrowDown", shift: true },
    // Déclenchés par le menu Édition (voir `acceleratorFor`).
    "selection.all": { key: "a", primary: true },
    "selection.none": { key: "d", primary: true },
    "view.grid": { key: "g" },
    "view.loupe": { key: "e" },
    "view.toggleLoupe": { key: " " },
    "view.cull": { key: "f" },
    "view.compare": { key: "c" },
    "view.survey": { key: "n" },
    // Menu Affichage seulement : Lightroom n'a pas de touche pour la carte
    // dans la Bibliothèque.
    "view.map": null,
    // Menu Affichage seulement : F6, celle de Lightroom, est souvent prise
    // par macOS.
    "view.toggleFilmstrip": null,
    // Mêmes touches que « photo du dessus / du dessous » de la Grille : la
    // vue affichée décide (voir `findShortcuts`).
    "compare.choose": { key: "ArrowUp" },
    "compare.swap": { key: "ArrowDown" },
    "view.back": { key: "Escape" },
    // Touche physique « \ » d'un clavier QWERTY (« ` » sur un AZERTY Mac).
    "filters.toggleBar": { key: "\\", code: "Backslash" },
    // Déclenché par le menu Affichage (voir `acceleratorFor`).
    "filters.toggleEnabled": { key: "l", primary: true },
    "loupe.toggleZoom": { key: "z" },
    "flag.picked": { key: "p" },
    "flag.rejected": { key: "x" },
    "flag.none": { key: "u" },
    "rating.0": digit(0),
    "rating.1": digit(1),
    "rating.2": digit(2),
    "rating.3": digit(3),
    "rating.4": digit(4),
    "rating.5": digit(5),
    "color.red": digit(6),
    "color.yellow": digit(7),
    "color.green": digit(8),
    "color.blue": digit(9),
    "color.purple": null,
    "color.none": null,
    // Touches physiques des crochets d'un clavier QWERTY (« ^ » et « $ » sur
    // un AZERTY Mac) : le geste de Lightroom, sans Option ni Maj.
    "photo.deleteRejected": { key: "Backspace", primary: true },
    // Dans une collection : retirer de la collection, comme Lightroom ; ailleurs, la même touche ouvre « Supprimer les
    // photos ».
    "photo.removeFromCollection": { key: "Backspace" },
    // Suppr (Fn + ⌫ sur un Mac).
    "photo.delete": { key: "Delete" },
    // F2, comme dans Lightroom.
    "photo.rename": { key: "F2" },
    // Retouche dans l'éditeur par défaut : Cmd+E, comme
    // « Modifier dans Photoshop » de Lightroom ; Cmd+R, « Afficher dans le
    // Finder », comme Lightroom.
    "photo.editExternal": { key: "e", code: "KeyE", primary: true },
    "photo.revealInFinder": { key: "r", code: "KeyR", primary: true },
    // Collection rapide et collection cible. Option change le
    // caractère de B sur un Mac : la touche physique le reconnaît.
    "collection.toggleTarget": { key: "b", code: "KeyB" },
    "collection.showQuick": { key: "b", code: "KeyB", primary: true },
    "collection.clearQuick": {
      key: "b",
      code: "KeyB",
      primary: true,
      shift: true,
    },
    "collection.saveQuick": {
      key: "b",
      code: "KeyB",
      primary: true,
      alt: true,
    },
    // Piles : les touches de Lightroom.
    "stack.group": { key: "g", code: "KeyG", primary: true },
    "stack.ungroup": { key: "g", code: "KeyG", primary: true, shift: true },
    "stack.toggle": { key: "s", code: "KeyS" },
    // Menu Photo seulement : un résumé à confirmer s'ouvre.
    "stack.autoPairs": null,
    "stack.autoBursts": null,
    "photo.rotateLeft": { key: "[", code: "BracketLeft", primary: true },
    "photo.rotateRight": { key: "]", code: "BracketRight", primary: true },
    // Menu Métadonnées seulement, comme dans Lightroom.
    "metadata.readFromFiles": null,
    // Déclenché par le menu Métadonnées (voir `acceleratorFor`), comme
    // l'enregistrement de Lightroom.
    "metadata.saveToFiles": { key: "s", primary: true },
    "metadata.removeLocation": null,
    // Menu Bibliothèque seulement, comme dans Lightroom.
    "library.findMissing": null,
    "library.operationsLog": null,
    "keywordSet.1": altDigit(1),
    "keywordSet.2": altDigit(2),
    "keywordSet.3": altDigit(3),
    "keywordSet.4": altDigit(4),
    "keywordSet.5": altDigit(5),
    "keywordSet.6": altDigit(6),
    "keywordSet.7": altDigit(7),
    "keywordSet.8": altDigit(8),
    "keywordSet.9": altDigit(9),
    // Déclenchés par le menu Édition (voir `acceleratorFor`).
    "edit.undo": { key: "z", primary: true },
    "edit.redo": { key: "z", primary: true, shift: true },
    // Déclenché par la barre de menus (voir `acceleratorFor`).
    "app.preferences": { key: ",", primary: true },
  };

/** Système de l'utilisateur, pour la touche de commande principale. */
export function isApplePlatform(userAgent = navigator.userAgent): boolean {
  return /Mac|iPhone|iPad/.test(userAgent);
}

/** Touches de modification enfoncées pendant un appui. */
export type PressedModifiers = Pick<
  KeyboardEvent,
  "key" | "metaKey" | "ctrlKey" | "shiftKey" | "altKey"
> &
  Partial<Pick<KeyboardEvent, "code">>;

/**
 * Indique si un appui correspond exactement à une combinaison : une touche
 * de modification en trop (Cmd+G pour « G ») ne correspond pas.
 */
export function matchesBinding(
  binding: KeyBinding,
  event: PressedModifiers,
  isApple: boolean,
): boolean {
  const primaryPressed = isApple ? event.metaKey : event.ctrlKey;
  // Ctrl sur macOS, ou la touche Windows, n'a pas de rôle ici.
  const otherPressed = isApple ? event.ctrlKey : event.metaKey;
  const keyMatches =
    event.key.toLowerCase() === binding.key.toLowerCase() ||
    (binding.code !== undefined && event.code === binding.code);
  return (
    keyMatches &&
    primaryPressed === (binding.primary ?? false) &&
    event.shiftKey === (binding.shift ?? false) &&
    event.altKey === (binding.alt ?? false) &&
    !otherPressed
  );
}

/**
 * Raccourci au format de la barre de menus native de Tauri (« CmdOrCtrl+, »),
 * pour les commandes déclenchées depuis un menu : le système affiche alors
 * le raccourci à côté de la commande.
 */
export function acceleratorFor(binding: KeyBinding): string {
  const parts = [
    binding.primary === true ? "CmdOrCtrl" : undefined,
    binding.shift === true ? "Shift" : undefined,
    binding.alt === true ? "Alt" : undefined,
    binding.key.length === 1 ? binding.key.toUpperCase() : binding.key,
  ];
  return parts.filter((part) => part !== undefined).join("+");
}

/**
 * Raccourci tel qu'on l'affiche à l'écran : « ⌥1 » sur macOS, « Alt+1 »
 * sous Windows.
 */
export function shortcutLabel(binding: KeyBinding, isApple: boolean): string {
  const key =
    binding.key.length === 1 ? binding.key.toUpperCase() : binding.key;
  const modifiers = isApple
    ? [
        binding.primary === true ? "⌘" : "",
        binding.alt === true ? "⌥" : "",
        binding.shift === true ? "⇧" : "",
      ]
    : [
        binding.primary === true ? "Ctrl+" : "",
        binding.alt === true ? "Alt+" : "",
        binding.shift === true ? "Shift+" : "",
      ];
  return `${modifiers.join("")}${key}`;
}

/**
 * Actions correspondant à un appui, dans l'ordre du registre. Une même
 * touche peut servir à plusieurs actions de vues différentes (↑ : photo du
 * dessus dans la Grille, « Choisir » en Comparaison) : la vue affichée
 * décide, selon celles qu'elle gère.
 */
export function findShortcuts(
  event: PressedModifiers,
  bindings: Readonly<Record<ShortcutId, KeyBinding | null>>,
  isApple: boolean,
): ShortcutId[] {
  return (Object.keys(bindings) as ShortcutId[]).filter((id) => {
    const binding = bindings[id];
    return binding !== null && matchesBinding(binding, event, isApple);
  });
}

/** Première action correspondant à un appui, s'il y en a une. */
export function findShortcut(
  event: PressedModifiers,
  bindings: Readonly<Record<ShortcutId, KeyBinding | null>>,
  isApple: boolean,
): ShortcutId | undefined {
  return findShortcuts(event, bindings, isApple)[0];
}

/**
 * Raccourci d'une action pour la barre de menus, s'il en a un et s'il
 * comporte une touche de commande : un raccourci d'une seule lettre (P, 3…)
 * déclaré dans un menu natif empêcherait de taper cette lettre dans les
 * champs de saisie.
 */
export function menuAccelerator(id: ShortcutId): string | undefined {
  const binding = DEFAULT_BINDINGS[id];
  // Cmd+Retour arrière efface une ligne dans un champ de saisie : déclaré
  // dans un menu natif, il l'en empêcherait.
  const editsText = binding?.key === "Backspace" || binding?.key === "Delete";
  return binding?.primary === true && !editsText
    ? acceleratorFor(binding)
    : undefined;
}
