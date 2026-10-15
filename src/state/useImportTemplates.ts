import { useEffect, useState } from "react";

import { fetchMetadataTemplates } from "../services/metadataTemplates";
import type { MetadataTemplateDto } from "../services/metadataTemplates";

/**
 * Les modèles de métadonnées proposés à l'import, lus à
 * l'ouverture de la fenêtre d'import : `null` tant qu'ils ne sont pas lus.
 * S'ils sont illisibles, la liste est vide (l'import reste possible, sans
 * modèle).
 *
 * @param isWanted faux : rien n'est lu (import impossible).
 */
export function useImportTemplates(
  isWanted: boolean,
): readonly MetadataTemplateDto[] | null {
  const [templates, setTemplates] = useState<MetadataTemplateDto[] | null>(
    null,
  );

  useEffect(() => {
    if (!isWanted) return;
    let isActive = true;
    fetchMetadataTemplates().then(
      (list) => {
        if (isActive) setTemplates(list);
      },
      (error: unknown) => {
        console.error("Failed to read the metadata templates", error);
        if (isActive) setTemplates([]);
      },
    );
    return () => {
      isActive = false;
    };
  }, [isWanted]);

  return templates;
}
