import { commands } from "./bindings";
import type {
  ExternalEditFailure,
  ExternalEditOutcomeDto,
  ExternalEditPreviewDto,
  ExternalEditorDto,
  ExternalEditorsDto,
  ExternalEditorsFailure,
  RevealFailure,
} from "./bindings";

export type {
  ExternalEditFailure,
  ExternalEditOutcomeDto,
  ExternalEditPreviewDto,
  ExternalEditorDto,
  ExternalEditorsDto,
  ExternalEditorsFailure,
  RevealFailure,
};

/** Résultat d'un appel qui peut être refusé. */
export type Outcome<T, E> =
  { status: "ok"; data: T } | { status: "error"; error: E };

/**
 * Éditeurs externes proposés : ceux trouvés dans Applications,
 * puis les applications ajoutées, et l'éditeur de Cmd+E.
 *
 * @throws si le cœur ne répond pas.
 */
export function fetchExternalEditors(): Promise<ExternalEditorsDto> {
  return commands.getExternalEditors();
}

/**
 * Fait choisir une application à ajouter aux éditeurs (elle devient celle
 * de Cmd+E) ; `null` dans `data` si l'utilisateur renonce.
 *
 * @throws si le cœur ne répond pas.
 */
export function addExternalEditor(
  title: string,
): Promise<Outcome<ExternalEditorsDto | null, ExternalEditorsFailure>> {
  return commands.addExternalEditor(title);
}

/**
 * Retire une application ajoutée.
 *
 * @throws si le cœur ne répond pas.
 */
export function removeExternalEditor(
  id: string,
): Promise<Outcome<ExternalEditorsDto, ExternalEditorsFailure>> {
  return commands.removeExternalEditor(id);
}

/**
 * Choisit l'éditeur de Cmd+E.
 *
 * @throws si le cœur ne répond pas.
 */
export function setDefaultExternalEditor(
  id: string,
): Promise<Outcome<ExternalEditorsDto, ExternalEditorsFailure>> {
  return commands.setDefaultExternalEditor(id);
}

/**
 * Ce que « Modifier dans… » ferait des photos : combien partent, combien
 * pourraient partir par une copie.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchExternalEditPreview(
  mediaIds: number[],
): Promise<ExternalEditPreviewDto> {
  const result = await commands.getExternalEditPreview(mediaIds);
  if (result.status === "error") {
    throw new Error(`External edit preview unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Ouvre les photos dans un éditeur (`null` : celui de Cmd+E) ;
 * `copyNonRaw` : confier par une copie `-Modifié` celles qui ne sont pas
 * des RAW.
 *
 * @throws si le cœur ne répond pas.
 */
export function editInExternalEditor(
  mediaIds: number[],
  editorId: string | null,
  copyNonRaw: boolean,
): Promise<Outcome<ExternalEditOutcomeDto, ExternalEditFailure>> {
  return commands.editInExternalEditor(mediaIds, editorId, copyNonRaw);
}

/**
 * Montre le fichier d'une photo dans le Finder.
 *
 * @throws si le cœur ne répond pas.
 */
export function revealInFinder(
  mediaId: number,
): Promise<Outcome<null, RevealFailure>> {
  return commands.revealInFinder(mediaId);
}
