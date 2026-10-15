import { convertFileSrc } from "@tauri-apps/api/core";

import { commands } from "./bindings";
import type {
  GridColorChoice,
  GridFilter,
  GridFlagChoice,
  GridItem,
  GridKindChoice,
  GridRatingComparison,
  GridSort,
  GridSortKey,
  GridSource,
  GridTextField,
  GridTextRule,
  GridTextSearch,
  GridThumbnail,
  GridXmpState,
  MediaFileStatus,
} from "./bindings";

export type {
  GridColorChoice,
  GridFilter,
  GridFlagChoice,
  GridItem,
  GridKindChoice,
  GridRatingComparison,
  GridSort,
  GridSortKey,
  GridSource,
  GridTextField,
  GridTextRule,
  GridTextSearch,
  GridThumbnail,
  GridXmpState,
  MediaFileStatus,
};

/** Éléments à afficher dans la grille : une source, filtrée et triée. */
export interface GridQuery {
  source: GridSource;
  filter: GridFilter;
  sort: GridSort;
}

/** Nom du protocole qui sert les images (voir `protocol.rs`). */
const IMAGE_SCHEME = "piclib";

/**
 * Nombre d'éléments d'une source de la grille, filtrés.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchGridCount(query: GridQuery): Promise<number> {
  const result = await commands.getGridCount(query.source, query.filter);
  if (result.status === "error") {
    throw new Error(`Grid count unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Éléments `offset` à `offset + limit - 1` de la grille, filtrés et triés
 * (500 au plus par appel).
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchGridPage(
  query: GridQuery,
  offset: number,
  limit: number,
): Promise<GridItem[]> {
  const result = await commands.getGridPage(
    query.source,
    query.filter,
    query.sort,
    offset,
    limit,
  );
  if (result.status === "error") {
    throw new Error(`Grid page unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Identifiants des éléments `offset` à `offset + limit - 1` de la grille,
 * filtrés, dans son ordre (sélection d'une plage, ou de tout).
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchGridIds(
  query: GridQuery,
  offset: number,
  limit: number,
): Promise<number[]> {
  const result = await commands.getGridIds(
    query.source,
    query.filter,
    query.sort,
    offset,
    limit,
  );
  if (result.status === "error") {
    throw new Error(`Grid ids unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Indique si le fichier d'un média est présent, pour expliquer pourquoi il
 * ne s'affiche pas.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function checkMediaFile(
  mediaId: number,
): Promise<MediaFileStatus> {
  const result = await commands.checkMediaFile(mediaId);
  if (result.status === "error") {
    throw new Error(`Media file check unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Adresse d'une image servie par le protocole `piclib` :
 * `piclib://localhost/thumbnail/123` sur macOS,
 * `http://piclib.localhost/thumbnail/123` sous Windows. Tauri choisit la
 * forme selon le système ; seul le début de l'adresse lui est demandé, le
 * reste ne doit pas être encodé.
 *
 * `version` (`GridItem.imageVersion`) change quand le fichier est remplacé
 *(export réécrit par un éditeur) : la nouvelle adresse évite
 * que la vue web ne montre l'image qu'elle a gardée en mémoire.
 */
function imageUrl(kind: string, mediaId: number, version?: number): string {
  const url = `${convertFileSrc("", IMAGE_SCHEME)}${kind}/${String(mediaId)}`;
  return version === undefined ? url : `${url}?v=${String(version)}`;
}

/** Vignette d'un média (512 pixels). */
export function thumbnailUrl(mediaId: number, version?: number): string {
  return imageUrl("thumbnail", mediaId, version);
}

/** Aperçu d'un média pour la Loupe (2560 pixels), fabriqué au besoin. */
export function previewUrl(mediaId: number, version?: number): string {
  return imageUrl("preview", mediaId, version);
}

/** Photo en pleine résolution, pour le zoom à 100 %. */
export function originalUrl(mediaId: number, version?: number): string {
  return imageUrl("original", mediaId, version);
}

/**
 * Vignette d'un fichier d'une source analysée, dans l'écran d'import
 * (fabriquée au besoin à partir du fichier, qui est seulement lu).
 */
export function importThumbnailUrl(
  analysisJobId: number,
  index: number,
): string {
  return imageUrl(`import/${String(analysisJobId)}`, index);
}

/**
 * Modèle d'adresse des tuiles du fond de carte, servies par le cœur
 * : `{z}`, `{x}` et `{y}` sont remplacés par la carte.
 */
export function mapTileUrlTemplate(): string {
  return `${convertFileSrc("", IMAGE_SCHEME)}tile/{z}/{x}/{y}`;
}

/** Cherche de nouveau les fichiers de tout le catalogue (tâche de fond). */
export async function checkMissingFiles(): Promise<void> {
  await commands.checkMissingFiles();
}
