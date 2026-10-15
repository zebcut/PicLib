import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  exportKeywordList,
  previewKeywordImport,
} from "../services/keywordTransfer";
import type {
  KeywordExportDto,
  KeywordImportPreviewDto,
  KeywordTransferFailure,
} from "../services/keywordTransfer";

/** Fenêtre à afficher pendant un import ou un export de mots-clés. */
export type KeywordTransferDialogState =
  | { kind: "preview"; preview: KeywordImportPreviewDto }
  | { kind: "exported"; result: KeywordExportDto }
  /** `failure` à `null` : le cœur n'a pas répondu. */
  | { kind: "failed"; failure: KeywordTransferFailure | null };

/** Import et export de la liste des mots-clés, et leur fenêtre. */
export interface KeywordTransfer {
  dialog: KeywordTransferDialogState | null;
  /** Fait choisir un fichier, puis montre l'aperçu de l'import. */
  startImport: () => void;
  /** Fait choisir où enregistrer la liste, puis l'écrit. */
  startExport: () => void;
  /** Importe ce que montre l'aperçu, et ferme la fenêtre. */
  confirmImport: () => void;
  close: () => void;
}

/**
 * Import et export de la liste des mots-clés au format texte de Lightroom
 * : fenêtres de choix du système, aperçu à confirmer avant
 * d'importer, message à la fin d'un export.
 *
 * @param importKeywords importe la liste de l'aperçu (annulable).
 */
export function useKeywordTransfer(
  importKeywords: () => void,
): KeywordTransfer {
  const { t } = useTranslation();
  const [dialog, setDialog] = useState<KeywordTransferDialogState | null>(null);

  const unreachable = useCallback((error: unknown) => {
    console.error("Failed to reach the core", error);
    setDialog({ kind: "failed", failure: null });
  }, []);

  const startImport = useCallback(() => {
    previewKeywordImport({
      title: t("keywordTransfer.importDialogTitle"),
      filterName: t("keywordTransfer.filterName"),
    }).then((result) => {
      if (result.status === "failed") {
        setDialog({ kind: "failed", failure: result.failure });
      } else if (result.value !== null) {
        setDialog({ kind: "preview", preview: result.value });
      }
    }, unreachable);
  }, [t, unreachable]);

  const startExport = useCallback(() => {
    exportKeywordList({
      title: t("keywordTransfer.exportDialogTitle"),
      filterName: t("keywordTransfer.filterName"),
      defaultName: t("keywordTransfer.defaultName"),
    }).then((result) => {
      if (result.status === "failed") {
        setDialog({ kind: "failed", failure: result.failure });
      } else if (result.value !== null) {
        setDialog({ kind: "exported", result: result.value });
      }
    }, unreachable);
  }, [t, unreachable]);

  const confirmImport = useCallback(() => {
    importKeywords();
    setDialog(null);
  }, [importKeywords]);

  const close = useCallback(() => {
    setDialog(null);
  }, []);

  return { dialog, startImport, startExport, confirmImport, close };
}
