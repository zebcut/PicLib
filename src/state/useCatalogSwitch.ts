import { useCallback, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import * as selection from "../services/catalogSelection";
import type { CatalogSelectionFailure } from "../services/catalogSelection";

/** Échec d'un changement de catalogue, y compris un cœur qui ne répond pas. */
export type CatalogSwitchFailure =
  CatalogSelectionFailure | { kind: "coreUnreachable" };

/** Étape en cours d'un changement de catalogue. */
export type CatalogSwitchState =
  | { step: "idle" }
  /** Fenêtre du système ouverte, ou redémarrage en cours. */
  | { step: "busy" }
  | { step: "alreadyOpen" }
  | { step: "syncedLocation"; path: string; suggestedPath: string }
  | { step: "failed"; failure: CatalogSwitchFailure };

/** Actions et état du changement de catalogue. */
export interface CatalogSwitch {
  state: CatalogSwitchState;
  /** « Ouvrir un catalogue… » */
  openCatalog: () => void;
  /** « Nouveau catalogue… » */
  createCatalog: () => void;
  /** Réponse à l'avertissement « emplacement synchronisé ». */
  confirmCreation: (addNosyncSuffix: boolean) => void;
  /** Ferme le message affiché. */
  dismiss: () => void;
}

/** Pilote l'ouverture et la création d'un catalogue. */
export function useCatalogSwitch(): CatalogSwitch {
  const { t } = useTranslation();
  const [state, setState] = useState<CatalogSwitchState>({ step: "idle" });
  // Empêche d'ouvrir une seconde fenêtre de sélection (menu cliqué deux fois).
  const isBusy = useRef(false);

  const run = useCallback((action: () => Promise<CatalogSwitchState>) => {
    if (isBusy.current) return;
    isBusy.current = true;
    setState({ step: "busy" });
    action()
      .then(setState, (error: unknown) => {
        console.error("Catalog switch failed", error);
        setState({ step: "failed", failure: { kind: "coreUnreachable" } });
      })
      .finally(() => {
        isBusy.current = false;
      });
  }, []);

  const openCatalog = useCallback(() => {
    run(async () => {
      const result = await selection.openCatalog(
        t("catalogSwitch.openDialogTitle"),
      );
      if (result.status === "error") {
        return { step: "failed", failure: result.error };
      }
      return result.data.outcome === "alreadyOpen"
        ? { step: "alreadyOpen" }
        : { step: "idle" };
    });
  }, [run, t]);

  const createCatalog = useCallback(() => {
    run(async () => {
      const result = await selection.createCatalog(
        t("catalogSwitch.createDialogTitle"),
        t("catalogSwitch.defaultCatalogName"),
      );
      if (result.status === "error") {
        return { step: "failed", failure: result.error };
      }
      const outcome = result.data;
      return outcome.outcome === "syncedLocation"
        ? {
            step: "syncedLocation",
            path: outcome.path,
            suggestedPath: outcome.suggestedPath,
          }
        : { step: "idle" };
    });
  }, [run, t]);

  const confirmCreation = useCallback(
    (addNosyncSuffix: boolean) => {
      run(async () => {
        const result = await selection.confirmCatalogCreation(addNosyncSuffix);
        return result.status === "error"
          ? { step: "failed", failure: result.error }
          : { step: "idle" };
      });
    },
    [run],
  );

  const dismiss = useCallback(() => {
    setState({ step: "idle" });
  }, []);

  return useMemo(
    () => ({ state, openCatalog, createCatalog, confirmCreation, dismiss }),
    [state, openCatalog, createCatalog, confirmCreation, dismiss],
  );
}
