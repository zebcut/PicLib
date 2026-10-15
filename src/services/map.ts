import { commands } from "./bindings";
import type {
  GridFilter,
  GridSource,
  LocationPreviewDto,
  MapAreaDto,
} from "./bindings";
import { toEditResult } from "./edits";
import type { EditResult } from "./edits";

export type { LocationPreviewDto };

/** Zone de la carte, en degrés. */
export interface MapArea {
  south: number;
  west: number;
  north: number;
  east: number;
}

/** Groupe de photos proches. */
export interface MapCluster {
  latitude: number;
  longitude: number;
  count: number;
  /** Une photo du groupe. */
  coverId: number;
  /** Zone couverte par ses photos. */
  area: MapArea;
}

/** Ce que la vue Carte affiche. */
export interface MapViewData {
  clusters: MapCluster[];
  locatedCount: number;
  unlocatedCount: number;
}

/**
 * Nombre reçu du cœur : les types générés autorisent `null` pour tout
 * nombre décimal (une valeur infinie s'écrirait ainsi en JSON), ce que le
 * cœur n'envoie jamais pour une position.
 */
function degrees(value: number | null): number {
  return value ?? 0;
}

function area(dto: MapAreaDto): MapArea {
  return {
    south: degrees(dto.south),
    west: degrees(dto.west),
    north: degrees(dto.north),
    east: degrees(dto.east),
  };
}

/**
 * Groupes de photos géolocalisées de la source filtrée dans `visible`, par
 * cases de `cellDegrees` degrés.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchMapView(
  source: GridSource,
  filter: GridFilter,
  visible: MapArea,
  cellDegrees: number,
): Promise<MapViewData> {
  const result = await commands.getMapView(
    source,
    filter,
    visible,
    cellDegrees,
  );
  if (result.status === "error") {
    throw new Error(`Map unavailable: ${result.error.kind}`);
  }
  return {
    clusters: result.data.clusters.map((cluster) => ({
      latitude: degrees(cluster.latitude),
      longitude: degrees(cluster.longitude),
      count: cluster.count,
      coverId: cluster.coverId,
      area: area(cluster.area),
    })),
    locatedCount: result.data.locatedCount,
    unlocatedCount: result.data.unlocatedCount,
  };
}

/**
 * Photos de la source filtrée situées dans `zone` (les photos d'un groupe).
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchMapAreaIds(
  source: GridSource,
  filter: GridFilter,
  zone: MapArea,
): Promise<number[]> {
  const result = await commands.getMapAreaIds(source, filter, zone);
  if (result.status === "error") {
    throw new Error(`Map photos unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Combien des photos `mediaIds` ont déjà une position ou un lieu (à
 * confirmer avant de les remplacer).
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchLocationPreview(
  mediaIds: number[],
): Promise<LocationPreviewDto> {
  const result = await commands.getLocationPreview(mediaIds);
  if (result.status === "error") {
    throw new Error(`Location preview unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Donne une position aux photos, avec le pays, la région et la ville du
 * lieu le plus proche, de façon annulable ; ce qu'elles avaient est
 * remplacé.
 *
 * @throws si le cœur ne répond pas.
 */
export async function setLocation(
  mediaIds: number[],
  latitude: number,
  longitude: number,
): Promise<EditResult> {
  return toEditResult(
    await commands.setLocation(mediaIds, latitude, longitude),
  );
}

/**
 * Retire la position des photos, de façon annulable (leur lieu est gardé).
 *
 * @throws si le cœur ne répond pas.
 */
export async function removeLocation(mediaIds: number[]): Promise<EditResult> {
  return toEditResult(await commands.removeLocation(mediaIds));
}
