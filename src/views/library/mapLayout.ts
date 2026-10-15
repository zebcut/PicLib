import type { MapArea, MapCluster } from "../../services/map";

/** Côté d'une tuile de carte, en pixels. */
const TILE_PIXELS = 256;

/** Écart à partir duquel deux photos forment des groupes séparés, en pixels. */
const CLUSTER_PIXELS = 64;

/** Zoom à partir duquel un clic sur un groupe sélectionne ses photos. */
export const SELECT_ZOOM = 16;

/**
 * Côté des cases de regroupement au zoom `zoom`, en degrés : environ
 * `CLUSTER_PIXELS` pixels à l'écran.
 */
export function cellDegrees(zoom: number): number {
  return (360 / 2 ** zoom) * (CLUSTER_PIXELS / TILE_PIXELS);
}

/**
 * Ce que fait un clic sur un groupe : zoomer dessus, ou sélectionner ses
 * photos quand elles sont au même endroit, ou que la carte est déjà de près.
 */
export function clusterAction(
  cluster: MapCluster,
  zoom: number,
): "zoom" | "select" {
  const { area } = cluster;
  const isOnePlace =
    Math.abs(area.north - area.south) < 1e-6 &&
    Math.abs(area.east - area.west) < 1e-6;
  return cluster.count === 1 || isOnePlace || zoom >= SELECT_ZOOM
    ? "select"
    : "zoom";
}

/** Diamètre d'un groupe à l'écran : plus grand pour plus de photos. */
export function clusterSize(count: number): number {
  return Math.round(28 + Math.min(24, Math.log10(Math.max(1, count)) * 10));
}

/** Zone qui contient tous les groupes, `null` s'il n'y en a pas. */
export function areaOfClusters(
  clusters: readonly MapCluster[],
): MapArea | null {
  if (clusters.length === 0) return null;
  return clusters.reduce<MapArea>(
    (area, cluster) => ({
      south: Math.min(area.south, cluster.area.south),
      west: Math.min(area.west, cluster.area.west),
      north: Math.max(area.north, cluster.area.north),
      east: Math.max(area.east, cluster.area.east),
    }),
    { south: 90, west: 180, north: -90, east: -180 },
  );
}
