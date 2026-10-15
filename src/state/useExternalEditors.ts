import { useCallback, useEffect, useMemo, useState } from "react";

import {
  addExternalEditor,
  fetchExternalEditors,
  removeExternalEditor,
  setDefaultExternalEditor,
} from "../services/externalEditors";
import type {
  ExternalEditorsDto,
  ExternalEditorsFailure,
  Outcome,
} from "../services/externalEditors";

/** Éditeurs externes et leurs réglages. */
export interface ExternalEditors {
  /** Éditeurs proposés ; `null` tant qu'ils ne sont pas lus. */
  state: ExternalEditorsDto | null;
  /** Relit la liste (une application a pu être installée). */
  reload: () => void;
  /** Fait choisir une application à ajouter ; vrai si elle l'a été. */
  add: (title: string) => Promise<boolean>;
  /** Retire une application ajoutée. */
  remove: (id: string) => Promise<void>;
  /** Choisit l'éditeur de Cmd+E. */
  setDefault: (id: string) => Promise<void>;
}

/** Lit les éditeurs externes et relaie leurs changements. */
export function useExternalEditors(): ExternalEditors {
  const [state, setState] = useState<ExternalEditorsDto | null>(null);

  const reload = useCallback(() => {
    fetchExternalEditors().then(setState, (error: unknown) => {
      console.error("Failed to read the external editors", error);
    });
  }, []);
  useEffect(reload, [reload]);

  const apply = useCallback(
    async (
      request: Promise<
        Outcome<ExternalEditorsDto | null, ExternalEditorsFailure>
      >,
    ): Promise<boolean> => {
      const result = await request;
      if (result.status === "error") {
        console.error("Failed to change the external editors", result.error);
        return false;
      }
      if (result.data === null) return false;
      setState(result.data);
      return true;
    },
    [],
  );
  const add = useCallback(
    (title: string) => apply(addExternalEditor(title)),
    [apply],
  );
  const remove = useCallback(
    async (id: string) => {
      await apply(removeExternalEditor(id));
    },
    [apply],
  );
  const setDefault = useCallback(
    async (id: string) => {
      await apply(setDefaultExternalEditor(id));
    },
    [apply],
  );

  return useMemo(
    () => ({ state, reload, add, remove, setDefault }),
    [state, reload, add, remove, setDefault],
  );
}
