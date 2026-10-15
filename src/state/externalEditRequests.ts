import { useEffect, useRef } from "react";

/**
 * Demandes « Modifier dans ▸ un éditeur » venues du menu Photo, qui ne
 * connaît pas la sélection : la bibliothèque affichée les reçoit et les
 * applique à ses photos sélectionnées. `null` : l'éditeur de Cmd+E.
 */
type Listener = (editorId: string | null) => void;

const listeners = new Set<Listener>();

/** Demande d'ouvrir la sélection dans l'éditeur `editorId`. */
export function requestExternalEdit(editorId: string | null): void {
  for (const listener of listeners) listener(editorId);
}

/** Reçoit les demandes tant que le composant est affiché. */
export function useExternalEditRequests(handler: Listener): void {
  const latest = useRef(handler);
  useEffect(() => {
    latest.current = handler;
  });
  useEffect(() => {
    const listener: Listener = (editorId) => {
      latest.current(editorId);
    };
    listeners.add(listener);
    return () => {
      listeners.delete(listener);
    };
  }, []);
}
