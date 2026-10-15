import "leaflet/dist/leaflet.css";

import L from "leaflet";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import {
  dropTargetAttributes,
  isHovered,
  useDragState,
  useDropHandler,
} from "../../components/DragProvider";
import { counted } from "../../i18n/format";
import { mapTileUrlTemplate } from "../../services/library";
import type { GridFilter, GridSource } from "../../services/library";
import { fetchMapAreaIds, fetchMapView } from "../../services/map";
import type { MapArea, MapCluster, MapViewData } from "../../services/map";
import {
  areaOfClusters,
  cellDegrees,
  clusterAction,
  clusterSize,
} from "./mapLayout";
import styles from "./MapView.module.css";

/** Clé de la dernière position de la carte, retenue d'une session à l'autre. */
const POSITION_STORAGE_KEY = "piclib.map.position";

/** Zoom maximal (celui des tuiles d'OpenStreetMap). */
const MAX_ZOOM = 19;

interface StoredPosition {
  latitude: number;
  longitude: number;
  zoom: number;
}

function storedPosition(): StoredPosition | null {
  try {
    const text = localStorage.getItem(POSITION_STORAGE_KEY);
    if (text === null) return null;
    const value = JSON.parse(text) as Partial<StoredPosition>;
    return typeof value.latitude === "number" &&
      typeof value.longitude === "number" &&
      typeof value.zoom === "number"
      ? (value as StoredPosition)
      : null;
  } catch {
    return null;
  }
}

function storePosition(map: L.Map): void {
  try {
    const center = map.getCenter();
    localStorage.setItem(
      POSITION_STORAGE_KEY,
      JSON.stringify({
        latitude: center.lat,
        longitude: center.lng,
        zoom: map.getZoom(),
      }),
    );
  } catch {
    // Mémoire du navigateur indisponible : la carte repartira du monde.
  }
}

/** Zone visible de la carte. */
function visibleArea(map: L.Map): MapArea {
  const bounds = map.getBounds();
  return {
    south: bounds.getSouth(),
    west: bounds.getWest(),
    north: bounds.getNorth(),
    east: bounds.getEast(),
  };
}

function boundsOf(area: MapArea): L.LatLngBoundsExpression {
  return [
    [area.south, area.west],
    [area.north, area.east],
  ];
}

/**
 * Clic sur un groupe : zoomer dessus, ou sélectionner ses photos.
 */
function openCluster(
  map: L.Map,
  cluster: MapCluster,
  latest: {
    source: GridSource;
    filter: GridFilter;
    onSelect: (mediaIds: number[]) => void;
  },
): void {
  if (clusterAction(cluster, map.getZoom()) === "zoom") {
    map.fitBounds(boundsOf(cluster.area), {
      padding: [40, 40],
      maxZoom: MAX_ZOOM,
    });
    return;
  }
  const { source: shownSource, filter: shownFilter, onSelect: select } = latest;
  fetchMapAreaIds(shownSource, shownFilter, cluster.area).then(
    select,
    (error: unknown) => {
      console.error("Failed to read the photos of a map group", error);
    },
  );
}

/**
 * Vue Carte : les photos géolocalisées de la
 * source affichée, filtres compris, regroupées par zone avec leur nombre.
 * Un clic sur un groupe zoome dessus, ou sélectionne ses photos (dans la
 * filmstrip) quand elles sont au même endroit ou que la carte est de près.
 * Le fond de carte vient d'OpenStreetMap, par le cœur ; sans connexion, un
 * message le dit, le reste de l'application marche normalement.
 *
 * Des photos glissées de la filmstrip sur la carte reçoivent la position
 * du point lâché.
 *
 * @param refreshKey la carte est relue quand cette valeur change.
 * @param onSelect sélectionne les photos d'un groupe.
 * @param onLocate donne une position à des photos glissées sur la carte ;
 *   absent, la carte ne reçoit pas de photos.
 * @param isLimiting la grille est limitée à la zone visible.
 * @param onLimitChange limite la grille à cette zone, suivie quand la carte
 *   bouge ; `null` : plus de limite.
 */
export function MapView({
  source,
  filter,
  refreshKey,
  onSelect,
  onLocate,
  isLimiting = false,
  onLimitChange,
}: {
  source: GridSource;
  filter: GridFilter;
  refreshKey: number;
  onSelect: (mediaIds: number[]) => void;
  onLocate?: (mediaIds: number[], latitude: number, longitude: number) => void;
  isLimiting?: boolean;
  onLimitChange?: (area: MapArea | null) => void;
}) {
  const { t } = useTranslation();
  const containerRef = useRef<HTMLDivElement>(null);
  const mapRef = useRef<L.Map | null>(null);
  const markersRef = useRef<L.LayerGroup | null>(null);
  const [isOffline, setIsOffline] = useState(false);
  const [view, setView] = useState<MapViewData | null>(null);
  const [moves, setMoves] = useState(0);
  // Sans position retenue, la carte montre d'abord toutes les photos.
  const shouldFit = useRef(false);
  const latest = useRef({ source, filter, onSelect });
  useEffect(() => {
    latest.current = { source, filter, onSelect };
  });

  // Limite de la grille : elle suit la zone visible à chaque déplacement.
  useEffect(() => {
    const map = mapRef.current;
    if (!isLimiting || map === null) return;
    onLimitChange?.(visibleArea(map));
  }, [isLimiting, moves, onLimitChange]);

  // La carte est créée une fois, avec son fond servi par le cœur.
  useEffect(() => {
    const container = containerRef.current;
    if (container === null) return;
    const position = storedPosition();
    const map = L.map(container, {
      worldCopyJump: true,
      minZoom: 1,
      maxZoom: MAX_ZOOM,
      attributionControl: true,
    });
    map.attributionControl.setPrefix(false);
    if (position === null) map.setView([20, 0], 2);
    else map.setView([position.latitude, position.longitude], position.zoom);
    const tiles = L.tileLayer(mapTileUrlTemplate(), {
      maxZoom: MAX_ZOOM,
      attribution: t("map.attribution"),
    });
    tiles.on("tileerror", () => {
      setIsOffline(true);
    });
    tiles.on("tileload", () => {
      setIsOffline(false);
    });
    tiles.addTo(map);
    markersRef.current = L.layerGroup().addTo(map);
    map.on("moveend", () => {
      storePosition(map);
      setMoves((count) => count + 1);
    });
    mapRef.current = map;
    if (position === null) shouldFit.current = true;
    return () => {
      map.remove();
      mapRef.current = null;
      markersRef.current = null;
    };
  }, [t]);

  const filterKey = JSON.stringify(filter);
  const sourceKey = JSON.stringify(source);

  // Groupes de la zone visible, relus à chaque déplacement.
  useEffect(() => {
    const map = mapRef.current;
    if (map === null) return;
    let isActive = true;
    const { source: shownSource, filter: shownFilter } = latest.current;
    fetchMapView(
      shownSource,
      shownFilter,
      visibleArea(map),
      cellDegrees(map.getZoom()),
    ).then(
      (result) => {
        if (!isActive) return;
        setView(result);
        if (shouldFit.current) {
          shouldFit.current = false;
          const area = areaOfClusters(result.clusters);
          if (area !== null)
            map.fitBounds(boundsOf(area), { padding: [40, 40] });
        }
      },
      (error: unknown) => {
        console.error("Failed to read the map", error);
      },
    );
    return () => {
      isActive = false;
    };
  }, [moves, refreshKey, filterKey, sourceKey]);

  // Les groupes deviennent des repères, avec leur nombre.
  useEffect(() => {
    const map = mapRef.current;
    const markers = markersRef.current;
    if (map === null || markers === null) return;
    markers.clearLayers();
    for (const cluster of view?.clusters ?? []) {
      const size = clusterSize(cluster.count);
      const marker = L.marker([cluster.latitude, cluster.longitude], {
        icon: L.divIcon({
          className: styles.cluster,
          html: `<span>${String(cluster.count)}</span>`,
          iconSize: [size, size],
        }),
        title: t("map.clusterTitle", counted(cluster.count)),
        keyboard: true,
      });
      marker.on("click", () => {
        openCluster(map, cluster, latest.current);
      });
      marker.addTo(markers);
    }
  }, [view, t]);

  // Photos glissées de la filmstrip : la position du point lâché.
  const drag = useDragState();
  const isDropTarget = isHovered(drag, { kind: "map" });
  useDropHandler(["map"], {
    canDrop: (payload) => payload.kind === "photos" && onLocate !== undefined,
    drop: (payload, _target, point) => {
      const map = mapRef.current;
      const container = containerRef.current;
      if (payload.kind !== "photos" || map === null || container === null) {
        return;
      }
      const rect = container.getBoundingClientRect();
      const position = map.containerPointToLatLng([
        point.x - rect.left,
        point.y - rect.top,
      ]);
      onLocate?.(payload.mediaIds, position.lat, position.lng);
    },
  });

  return (
    <div
      className={[styles.view, isDropTarget ? styles.dropTarget : ""].join(" ")}
    >
      <div
        ref={containerRef}
        className={styles.map}
        role="application"
        aria-label={t("map.label")}
        {...(onLocate === undefined
          ? {}
          : dropTargetAttributes({ kind: "map" }))}
      />
      {isOffline && (
        <p className={styles.offline} role="status">
          {t("map.offline")}
        </p>
      )}
      <div className={styles.toolbar}>
        {view !== null && (
          <>
            <span>{t("map.located", counted(view.locatedCount))}</span>
            {view.unlocatedCount > 0 && (
              <span className={styles.secondary}>
                {t("map.unlocated", counted(view.unlocatedCount))}
              </span>
            )}
            {onLocate !== undefined && (
              <span className={styles.secondary}>{t("map.dropHint")}</span>
            )}
            {onLimitChange !== undefined && (
              <label className={styles.limit}>
                <input
                  type="checkbox"
                  checked={isLimiting}
                  onChange={(event) => {
                    const map = mapRef.current;
                    onLimitChange(
                      event.target.checked && map !== null
                        ? visibleArea(map)
                        : null,
                    );
                  }}
                />
                {t("map.limit")}
              </label>
            )}
          </>
        )}
      </div>
    </div>
  );
}
