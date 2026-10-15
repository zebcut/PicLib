import { useCallback, useEffect, useState } from "react";

import {
  changeMetadataTemplates,
  fetchMetadataTemplates,
} from "../services/metadataTemplates";
import type {
  MetadataTemplateDto,
  MetadataTemplateOperation,
  MetadataTemplateRefusal,
} from "../services/metadataTemplates";

/** Modèles de métadonnées, lus dans le cœur. */
export type MetadataTemplatesState =
  | { status: "loading" }
  | { status: "ready"; templates: MetadataTemplateDto[] }
  | { status: "failed" };

/** Les modèles de métadonnées, et de quoi les modifier. */
export interface MetadataTemplates {
  state: MetadataTemplatesState;
  /**
   * Enregistre ou supprime un modèle puis relit la liste ; renvoie la
   * raison d'un refus, ou `null`.
   *
   * @throws si le catalogue est illisible.
   */
  change: (
    operation: MetadataTemplateOperation,
  ) => Promise<MetadataTemplateRefusal | null>;
}

/**
 * Les modèles de métadonnées, relus quand `refreshKey` change
 * (une fusion ou une suppression de mot-clé touche leurs mots-clés) et
 * après chaque modification.
 *
 * @param onKeywordsCreated appelé après l'enregistrement d'un modèle : un
 *   mot-clé saisi a pu être créé.
 */
export function useMetadataTemplates(
  refreshKey: number,
  onKeywordsCreated: () => void,
): MetadataTemplates {
  const [state, setState] = useState<MetadataTemplatesState>({
    status: "loading",
  });
  const [reloads, setReloads] = useState(0);

  useEffect(() => {
    let isActive = true;
    fetchMetadataTemplates().then(
      (templates) => {
        if (isActive) setState({ status: "ready", templates });
      },
      (error: unknown) => {
        console.error("Failed to read the metadata templates", error);
        if (isActive) setState({ status: "failed" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [refreshKey, reloads]);

  const change = useCallback(
    async (operation: MetadataTemplateOperation) => {
      const refusal = await changeMetadataTemplates(operation);
      if (refusal === null) {
        setReloads((previous) => previous + 1);
        if (operation.kind === "save") onKeywordsCreated();
      }
      return refusal;
    },
    [onKeywordsCreated],
  );

  return { state, change };
}
