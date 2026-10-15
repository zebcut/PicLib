import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import type { ReactNode } from "react";

import { DEFAULT_BINDINGS, findShortcuts, isApplePlatform } from "./registry";
import type { ShortcutId } from "./registry";

/** Fonctions à appeler pour certaines actions. */
export type ShortcutHandlers = Partial<Record<ShortcutId, () => void>>;

/** Liste des composants qui écoutent des raccourcis, du plus ancien au plus récent. */
type Listeners = { current: ShortcutHandlers }[];

const ShortcutContext = createContext<Listeners | null>(null);

/**
 * Déclenche une action comme si son raccourci avait été tapé ; renvoie
 * `false` si aucun composant affiché ne la gère.
 */
export type ShortcutTrigger = (id: ShortcutId) => boolean;

/**
 * Seul écouteur clavier de l'application : il traduit chaque appui en action
 * du registre et la transmet au composant affiché le plus récemment qui la
 * gère. Rien n'est déclenché pendant la saisie d'un texte, dans un curseur,
 * ou quand une fenêtre de message est ouverte.
 */
export function ShortcutProvider({ children }: { children: ReactNode }) {
  const [listeners] = useState<Listeners>(() => []);

  useEffect(() => {
    const isApple = isApplePlatform();
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.defaultPrevented || event.isComposing) return;
      if (isTyping(event.target) || isModalOpen()) return;
      const ids = findShortcuts(event, DEFAULT_BINDINGS, isApple);
      const handler = latestHandler(listeners, ids);
      if (handler === undefined) return;
      event.preventDefault();
      handler();
    };
    window.addEventListener("keydown", onKeyDown);
    return () => {
      window.removeEventListener("keydown", onKeyDown);
    };
  }, [listeners]);

  return (
    <ShortcutContext.Provider value={listeners}>
      {children}
    </ShortcutContext.Provider>
  );
}

/**
 * Fonction qui déclenche une action du registre depuis ailleurs que le
 * clavier (la barre de menus, par exemple), avec les mêmes règles : rien
 * n'est déclenché quand une fenêtre de message est ouverte.
 */
export function useShortcutTrigger(): ShortcutTrigger {
  const listeners = useListeners();
  return useCallback(
    (id: ShortcutId) => {
      // Comme au clavier : rien derrière une fenêtre de message.
      if (isModalOpen()) return false;
      const handler = latestHandler(listeners, [id]);
      if (handler === undefined) return false;
      handler();
      return true;
    },
    [listeners],
  );
}

/**
 * Déclare les actions qu'un composant gère tant qu'il est affiché. Les
 * fonctions peuvent changer à chaque affichage : la dernière version est
 * toujours utilisée.
 */
export function useShortcuts(handlers: ShortcutHandlers): void {
  const listeners = useListeners();
  const latest = useRef(handlers);

  useEffect(() => {
    latest.current = handlers;
  });

  useEffect(() => {
    const listener = latest;
    listeners.push(listener);
    return () => {
      listeners.splice(listeners.indexOf(listener), 1);
    };
  }, [listeners]);
}

function useListeners(): Listeners {
  const listeners = useContext(ShortcutContext);
  if (listeners === null) {
    throw new Error("Shortcuts must be used inside a ShortcutProvider");
  }
  return listeners;
}

/**
 * Fonction du composant affiché le plus récemment qui gère l'une de ces
 * actions (la première qu'il gère, dans l'ordre donné).
 */
function latestHandler(
  listeners: Listeners,
  ids: readonly ShortcutId[],
): (() => void) | undefined {
  for (let index = listeners.length - 1; index >= 0; index -= 1) {
    const handlers = listeners[index]?.current;
    if (handlers === undefined) continue;
    for (const id of ids) {
      const handler = handlers[id];
      if (handler !== undefined) return handler;
    }
  }
  return undefined;
}

/** Vrai si l'appui est destiné à un champ (texte, curseur, liste...). */
function isTyping(target: EventTarget | null): boolean {
  if (!(target instanceof HTMLElement)) return false;
  return (
    target.isContentEditable ||
    target instanceof HTMLInputElement ||
    target instanceof HTMLTextAreaElement ||
    target instanceof HTMLSelectElement
  );
}

/** Vrai si une fenêtre de message bloque le reste de l'écran. */
function isModalOpen(): boolean {
  return document.querySelector('[aria-modal="true"]') !== null;
}
