import { commands } from "./bindings";
import type {
  KeywordExportDto,
  KeywordImportPreviewDto,
  KeywordLineIssueDto,
  KeywordLineIssueReasonDto,
  KeywordTransferFailure,
} from "./bindings";
import type { EditResult } from "./edits";

export type {
  KeywordExportDto,
  KeywordImportPreviewDto,
  KeywordLineIssueDto,
  KeywordLineIssueReasonDto,
  KeywordTransferFailure,
};

/** Textes des fenêtres de choix de fichier, traduits par l'interface. */
export interface KeywordFileDialogTexts {
  title: string;
  /** Nom du filtre des fichiers `.txt` (« Texte »). */
  filterName: string;
}

/** Résultat d'un appel : valeur, ou raison de l'échec. */
export type TransferResult<T> =
  | { status: "ok"; value: T }
  | { status: "failed"; failure: KeywordTransferFailure };

/**
 * Fait choisir un fichier de mots-clés et le compare à la liste : `null` si l'utilisateur ferme la fenêtre. Rien n'est importé.
 *
 * @throws si le cœur ne répond pas.
 */
export async function previewKeywordImport(
  texts: KeywordFileDialogTexts,
): Promise<TransferResult<KeywordImportPreviewDto | null>> {
  const result = await commands.previewKeywordImport(
    texts.title,
    texts.filterName,
  );
  return result.status === "ok"
    ? { status: "ok", value: result.data }
    : { status: "failed", failure: result.error };
}

/**
 * Importe la liste du dernier aperçu, de façon annulable.
 *
 * @throws si le cœur ne répond pas.
 */
export async function importKeywordList(): Promise<EditResult> {
  const result = await commands.importKeywords();
  if (result.status === "ok") return { status: "ok", outcome: result.data };
  // Rien à importer (aperçu perdu) : échec du catalogue pour l'utilisateur,
  // la cause est dans le journal de la console.
  console.error("Failed to import the keywords", result.error);
  return { status: "error", failure: { kind: "catalogUnavailable" } };
}

/**
 * Fait choisir où enregistrer la liste des mots-clés et l'y écrit : `null`
 * si l'utilisateur ferme la fenêtre.
 *
 * @throws si le cœur ne répond pas.
 */
export async function exportKeywordList(
  texts: KeywordFileDialogTexts & { defaultName: string },
): Promise<TransferResult<KeywordExportDto | null>> {
  const result = await commands.exportKeywords(
    texts.title,
    texts.filterName,
    texts.defaultName,
  );
  return result.status === "ok"
    ? { status: "ok", value: result.data }
    : { status: "failed", failure: result.error };
}
