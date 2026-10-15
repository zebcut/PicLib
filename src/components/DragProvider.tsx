import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useRef,
  useState,
} from "react";
import type { PointerEvent as ReactPointerEvent, ReactNode, Ref } from "react";

import { useShortcuts } from "../shortcuts/ShortcutProvider";
import styles from "./DragProvider.module.css";

/** Distance, en pixels, au-delà de laquelle un appui devient un glisser. */
const DRAG_THRESHOLD_PX = 4;

/** Bande, en pixels, près du bord d'une zone qui la fait défiler. */
const AUTOSCROLL_EDGE_PX = 28;

/** Défilement maximal par image, en pixels. */
const AUTOSCROLL_STEP_PX = 14;

/** Ce qu'on glisse. */
export type DragPayload =
  | { kind: "keyword"; keywordId: number; label: string }
  | { kind: "photos"; mediaIds: number[]; label: string }
  | { kind: "folder"; folderId: number; label: string }
  | { kind: "collection"; collectionId: number; label: string };

/** Là où on peut déposer. */
export type DropTarget =
  | { kind: "keyword"; keywordId: number }
  | { kind: "keywordRoot" }
  | { kind: "media"; mediaId: number }
  | { kind: "folder"; folderId: number }
  | { kind: "collection"; collectionId: number }
  | { kind: "collectionRoot" }
  | { kind: "map" };

/** Point de l'écran où le glisser est lâché. */
export interface DropPoint {
  x: number;
  y: number;
}

/** Ce qu'une zone accepte, et quoi faire au dépôt. */
export interface DropHandler {
  canDrop: (payload: DragPayload, target: DropTarget) => boolean;
  /** `point` : où le pointeur a été lâché (la carte en tire une position). */
  drop: (payload: DragPayload, target: DropTarget, point: DropPoint) => void;
}

/** Glisser en cours : ce qu'on glisse, et la cible acceptée survolée. */
export interface DragState {
  payload: DragPayload;
  hover: DropTarget | null;
  /** Position du pointeur au début du glisser (l'étiquette part de là). */
  at: { x: number; y: number };
}

interface DragContextValue {
  state: DragState | null;
  start: (event: ReactPointerEvent, payload: () => DragPayload | null) => void;
  handlers: Map<DropTarget["kind"], DropHandler>;
}

const DragContext = createContext<DragContextValue | null>(null);

/**
 * Attributs qui font d'un élément une cible de dépôt : l'élément survolé le
 * plus proche qui les porte est la cible (une ligne de l'arbre plutôt que
 * le fond de la liste).
 */
export function dropTargetAttributes(target: DropTarget): {
  "data-drop-kind": string;
  "data-drop-id"?: string;
} {
  switch (target.kind) {
    case "keyword":
      return {
        "data-drop-kind": target.kind,
        "data-drop-id": String(target.keywordId),
      };
    case "media":
      return {
        "data-drop-kind": target.kind,
        "data-drop-id": String(target.mediaId),
      };
    case "folder":
      return {
        "data-drop-kind": target.kind,
        "data-drop-id": String(target.folderId),
      };
    case "collection":
      return {
        "data-drop-kind": target.kind,
        "data-drop-id": String(target.collectionId),
      };
    case "keywordRoot":
    case "collectionRoot":
    case "map":
      return { "data-drop-kind": target.kind };
  }
}

/** Attribut d'une zone qui défile toute seule près de ses bords. */
export const AUTOSCROLL_ATTRIBUTE = { "data-drag-autoscroll": "" };

/**
 * Glisser-déposer au pointeur, sans le glisser-déposer du
 * navigateur : il marche de la même façon sur macOS et Windows, sans
 * gêner le dépôt de dossiers depuis le Finder. Un appui ne devient un
 * glisser qu'après quelques pixels (un clic reste un clic) ; une étiquette
 * suit le pointeur ; Échap annule ; les zones marquées défilent près de
 * leurs bords.
 */
export function DragProvider({ children }: { children: ReactNode }) {
  const [state, setState] = useState<DragState | null>(null);
  const [handlers] = useState(() => new Map<DropTarget["kind"], DropHandler>());
  const ghost = useRef<HTMLDivElement>(null);
  // Glisser en cours, lu par les écouteurs du pointeur sans attendre
  // l'affichage suivant.
  const current = useRef<DragState | null>(null);
  const stop = useRef<(() => void) | null>(null);

  const update = (next: DragState | null) => {
    current.current = next;
    setState(next);
  };

  const start = useCallback(
    (event: ReactPointerEvent, makePayload: () => DragPayload | null) => {
      if (event.button !== 0 || stop.current !== null) return;
      const origin = { x: event.clientX, y: event.clientY };
      let pointer = origin;
      let frame = 0;

      const targetAt = (x: number, y: number): DropTarget | null => {
        const drag = current.current;
        if (drag === null) return null;
        const element = document
          .elementFromPoint(x, y)
          ?.closest<HTMLElement>("[data-drop-kind]");
        const target =
          element === null || element === undefined
            ? null
            : parseTarget(element);
        if (target === null) return null;
        const handler = handlers.get(target.kind);
        return handler?.canDrop(drag.payload, target) === true ? target : null;
      };

      const autoscroll = () => {
        const zone = document
          .elementFromPoint(pointer.x, pointer.y)
          ?.closest<HTMLElement>("[data-drag-autoscroll]");
        if (zone !== null && zone !== undefined) {
          const box = zone.getBoundingClientRect();
          if (pointer.y < box.top + AUTOSCROLL_EDGE_PX) {
            zone.scrollTop -= AUTOSCROLL_STEP_PX;
          } else if (pointer.y > box.bottom - AUTOSCROLL_EDGE_PX) {
            zone.scrollTop += AUTOSCROLL_STEP_PX;
          }
        }
        frame = requestAnimationFrame(autoscroll);
      };

      const onMove = (move: PointerEvent) => {
        pointer = { x: move.clientX, y: move.clientY };
        let drag = current.current;
        if (drag === null) {
          const distance = Math.hypot(
            pointer.x - origin.x,
            pointer.y - origin.y,
          );
          if (distance < DRAG_THRESHOLD_PX) return;
          const payload = makePayload();
          if (payload === null) {
            finish();
            return;
          }
          drag = { payload, hover: null, at: pointer };
          update(drag);
          document.body.classList.add(styles.dragging ?? "");
          frame = requestAnimationFrame(autoscroll);
        }
        move.preventDefault();
        if (ghost.current !== null) {
          ghost.current.style.transform = `translate(${String(pointer.x)}px, ${String(pointer.y)}px)`;
        }
        const hover = targetAt(pointer.x, pointer.y);
        if (!sameTarget(hover, drag.hover)) update({ ...drag, hover });
      };

      const onUp = (up: PointerEvent) => {
        const drag = current.current;
        if (drag !== null) {
          const target = targetAt(up.clientX, up.clientY);
          // Le clic qui suit le relâchement ne doit pas changer la
          // sélection.
          const swallow = (click: MouseEvent) => {
            click.stopPropagation();
            click.preventDefault();
          };
          window.addEventListener("click", swallow, { capture: true });
          setTimeout(() => {
            window.removeEventListener("click", swallow, { capture: true });
          }, 0);
          if (target !== null) {
            handlers.get(target.kind)?.drop(drag.payload, target, {
              x: up.clientX,
              y: up.clientY,
            });
          }
        }
        finish();
      };

      const finish = () => {
        window.removeEventListener("pointermove", onMove);
        window.removeEventListener("pointerup", onUp);
        window.removeEventListener("pointercancel", finish);
        cancelAnimationFrame(frame);
        document.body.classList.remove(styles.dragging ?? "");
        stop.current = null;
        if (current.current !== null) update(null);
      };

      stop.current = finish;
      window.addEventListener("pointermove", onMove);
      window.addEventListener("pointerup", onUp);
      window.addEventListener("pointercancel", finish);
    },
    [handlers],
  );

  // Un glisser interrompu par la disparition du composant.
  useEffect(
    () => () => {
      stop.current?.();
    },
    [],
  );

  return (
    <DragContext.Provider value={{ state, start, handlers }}>
      {children}
      {state !== null && (
        <DragGhost
          ref={ghost}
          label={state.payload.label}
          at={state.at}
          onCancel={() => {
            stop.current?.();
          }}
        />
      )}
    </DragContext.Provider>
  );
}

/** Étiquette qui suit le pointeur ; Échap annule le glisser. */
function DragGhost({
  ref,
  label,
  at,
  onCancel,
}: {
  ref: Ref<HTMLDivElement>;
  label: string;
  at: { x: number; y: number };
  onCancel: () => void;
}) {
  // Déclarée par-dessus les autres vues, le temps du glisser.
  useShortcuts({ "view.back": onCancel });
  // Ensuite, c'est le pointeur qui déplace l'étiquette, sans réafficher.
  return (
    <div
      ref={ref}
      className={styles.ghost}
      style={{ transform: `translate(${String(at.x)}px, ${String(at.y)}px)` }}
      aria-hidden
    >
      {label}
    </div>
  );
}

/**
 * Fonction à appeler sur `pointerdown` pour qu'un élément puisse être
 * glissé ; `payload` n'est calculé qu'au début du glisser (`null` : rien à
 * glisser).
 */
export function useDragStart(): DragContextValue["start"] {
  return useDragContext().start;
}

/** Glisser en cours, s'il y en a un. */
export function useDragState(): DragState | null {
  return useDragContext().state;
}

/**
 * Déclare ce que cette partie de l'interface accepte pour les cibles de
 * type `kinds`, tant qu'elle est affichée.
 */
export function useDropHandler(
  kinds: readonly DropTarget["kind"][],
  handler: DropHandler,
): void {
  const { handlers } = useDragContext();
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  });
  const key = kinds.join(",");
  useEffect(() => {
    const stable: DropHandler = {
      canDrop: (payload, target) => latest.current.canDrop(payload, target),
      drop: (payload, target, point) => {
        latest.current.drop(payload, target, point);
      },
    };
    const declared = key.split(",") as DropTarget["kind"][];
    for (const kind of declared) handlers.set(kind, stable);
    return () => {
      for (const kind of declared) {
        if (handlers.get(kind) === stable) handlers.delete(kind);
      }
    };
  }, [handlers, key]);
}

/** Vrai si `target` est la cible survolée par le glisser en cours. */
export function isHovered(
  state: DragState | null,
  target: DropTarget,
): boolean {
  return sameTarget(state?.hover ?? null, target);
}

function useDragContext(): DragContextValue {
  const context = useContext(DragContext);
  if (context === null) {
    throw new Error("Dragging must be used inside a DragProvider");
  }
  return context;
}

function parseTarget(element: HTMLElement): DropTarget | null {
  const id = Number(element.dataset.dropId);
  switch (element.dataset.dropKind) {
    case "keyword":
      return Number.isInteger(id) ? { kind: "keyword", keywordId: id } : null;
    case "media":
      return Number.isInteger(id) ? { kind: "media", mediaId: id } : null;
    case "folder":
      return Number.isInteger(id) ? { kind: "folder", folderId: id } : null;
    case "collection":
      return Number.isInteger(id)
        ? { kind: "collection", collectionId: id }
        : null;
    case "keywordRoot":
      return { kind: "keywordRoot" };
    case "collectionRoot":
      return { kind: "collectionRoot" };
    case "map":
      return { kind: "map" };
    default:
      return null;
  }
}

function sameTarget(a: DropTarget | null, b: DropTarget | null): boolean {
  if (a === null || b === null) return a === b;
  if (a.kind !== b.kind) return false;
  if (a.kind === "keyword" && b.kind === "keyword") {
    return a.keywordId === b.keywordId;
  }
  if (a.kind === "media" && b.kind === "media") return a.mediaId === b.mediaId;
  if (a.kind === "folder" && b.kind === "folder") {
    return a.folderId === b.folderId;
  }
  if (a.kind === "collection" && b.kind === "collection") {
    return a.collectionId === b.collectionId;
  }
  return true;
}
