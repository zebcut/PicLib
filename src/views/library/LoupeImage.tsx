import type { TFunction } from "i18next";
import { useEffect, useLayoutEffect, useRef, useState } from "react";
import type { PointerEvent } from "react";
import { useTranslation } from "react-i18next";

import {
  checkMediaFile,
  originalUrl,
  previewUrl,
  thumbnailUrl,
} from "../../services/library";
import type { GridItem, MediaFileStatus } from "../../services/library";
import styles from "./LoupeView.module.css";
import {
  centeredRotation,
  isQuarterTurn,
  rotatedSize,
  rotationTransform,
} from "./rotation";
import type { PanFocus, PanLink } from "./panLink";
import {
  centerFocus,
  FIT,
  focusAt,
  fullSizeInCssPixels,
  scrollToCenter,
} from "./zoom";
import type { Zoom } from "./zoom";

/** Déplacement minimal, en pixels, pour qu'un clic devienne un glissement. */
const DRAG_THRESHOLD = 4;

type LoadState = "loading" | "ready" | "failed";

/**
 * Un élément en grand : son aperçu adapté à la fenêtre (la vignette sert de
 * brouillon pendant le chargement), ou la pleine résolution à 100 %.
 * Un clic zoome à l'endroit cliqué ; à 100 %, on se déplace en faisant
 * glisser la photo, et un clic revient à l'affichage adapté.
 *
 * @param panLink relie le déplacement à 100 % à celui d'autres photos
 *   (Comparaison).
 */
export function LoupeImage({
  item,
  zoom,
  onZoomChange,
  panLink,
}: {
  item: GridItem;
  zoom: Zoom;
  onZoomChange: (zoom: Zoom) => void;
  panLink?: PanLink;
}) {
  const { t } = useTranslation();
  const [preview, setPreview] = useState<LoadState>("loading");
  // État de la pleine résolution, attaché au passage à 100 % qui l'a
  // demandée : chaque nouveau passage repart de « en cours ».
  const [fullLoad, setFullLoad] = useState<{
    zoom: Zoom;
    state: LoadState;
  } | null>(null);
  const full: LoadState =
    fullLoad !== null && fullLoad.zoom === zoom ? fullLoad.state : "loading";
  const [problem, setProblem] = useState<MediaFileStatus | "unknown" | null>(
    null,
  );

  const onPreviewError = () => {
    setPreview("failed");
    checkMediaFile(item.id).then(setProblem, (error: unknown) => {
      console.error("Failed to check a media file", error);
      setProblem("unknown");
    });
  };

  const showFull = zoom.mode === "full" && full === "ready";
  const turned = isQuarterTurn(item.rotation);
  return (
    <div className={styles.imageArea}>
      {!showFull && (
        <div className={styles.fitLayer}>
          {preview !== "ready" && item.thumbnail === "ready" && (
            <img
              className={
                turned ? [styles.draft, styles.turned].join(" ") : styles.draft
              }
              style={{
                transform: turned
                  ? centeredRotation(item.rotation)
                  : rotationTransform(item.rotation),
              }}
              src={thumbnailUrl(item.id, item.imageVersion)}
              alt=""
              draggable={false}
            />
          )}
          {preview !== "failed" && (
            <img
              className={
                preview === "ready"
                  ? [styles.fitImage, turned ? styles.turned : undefined]
                      .filter(Boolean)
                      .join(" ")
                  : styles.hiddenImage
              }
              style={{ transform: rotationTransform(item.rotation) }}
              src={previewUrl(item.id, item.imageVersion)}
              alt={item.fileName}
              draggable={false}
              onLoad={() => {
                setPreview("ready");
              }}
              onError={onPreviewError}
              onClick={(event) => {
                onZoomChange(
                  focusAt(
                    event.currentTarget.getBoundingClientRect(),
                    event.clientX,
                    event.clientY,
                  ),
                );
              }}
            />
          )}
        </div>
      )}
      {zoom.mode === "full" && full !== "failed" && (
        <FullSizeImage
          item={item}
          zoom={zoom}
          isVisible={showFull}
          panLink={panLink}
          onLoad={() => {
            setFullLoad({ zoom, state: "ready" });
          }}
          onError={() => {
            setFullLoad({ zoom, state: "failed" });
          }}
          onClick={() => {
            onZoomChange(FIT);
          }}
        />
      )}
      <Status
        message={
          preview === "failed"
            ? problemMessage(problem, t)
            : zoom.mode === "full" && full === "failed"
              ? t("loupe.fullSizeFailed")
              : (preview === "loading" && zoom.mode === "fit") ||
                  (zoom.mode === "full" && full === "loading")
                ? t("loupe.loading")
                : null
        }
        isError={
          preview === "failed" || (zoom.mode === "full" && full === "failed")
        }
      />
    </div>
  );
}

/**
 * Photo à 100 %, dans une zone qu'on fait défiler en la faisant glisser.
 * Elle reste invisible tant qu'elle n'est pas chargée : l'aperçu reste
 * affiché en attendant.
 */
function FullSizeImage({
  item,
  zoom,
  isVisible,
  panLink,
  onLoad,
  onError,
  onClick,
}: {
  item: GridItem;
  zoom: Extract<Zoom, { mode: "full" }>;
  isVisible: boolean;
  panLink: PanLink | undefined;
  onLoad: () => void;
  onError: () => void;
  onClick: () => void;
}) {
  const scrollRef = useRef<HTMLDivElement>(null);
  // Taille de l'image à 100 %, avant rotation, puis une fois pivotée.
  const [size, setSize] = useState<{ width: number; height: number } | null>(
    null,
  );
  const box = size === null ? null : rotatedSize(size, item.rotation);
  // Fonction qui recentre cette photo, reconnue par le lien comme la source
  // d'un déplacement (pour ne pas se suivre elle-même).
  const followerRef = useRef<((focus: PanFocus) => void) | null>(null);
  const drag = useRef<{
    x: number;
    y: number;
    left: number;
    top: number;
    moved: boolean;
  } | null>(null);

  // Une fois la taille connue, place le point visé au centre : celui des
  // photos liées si l'une a déjà été déplacée, sinon le point cliqué. Il est
  // exprimé sur l'image telle qu'affichée (pivotée), comme la taille du
  // cadre : aucune conversion n'est nécessaire.
  const boxWidth = box?.width;
  const boxHeight = box?.height;
  useLayoutEffect(() => {
    const scroller = scrollRef.current;
    if (
      scroller === null ||
      boxWidth === undefined ||
      boxHeight === undefined ||
      !isVisible
    ) {
      return;
    }
    const center = (focus: PanFocus) => {
      scroller.scrollLeft = scrollToCenter(
        focus.x,
        boxWidth,
        scroller.clientWidth,
      );
      scroller.scrollTop = scrollToCenter(
        focus.y,
        boxHeight,
        scroller.clientHeight,
      );
    };
    center(panLink?.current ?? { x: zoom.focusX, y: zoom.focusY });
    followerRef.current = center;
    const stop = panLink?.follow(center);
    return () => {
      stop?.();
      followerRef.current = null;
    };
  }, [boxWidth, boxHeight, isVisible, zoom.focusX, zoom.focusY, panLink]);

  /** Annonce aux photos liées le point désormais au centre. */
  const publishPan = (scroller: HTMLDivElement) => {
    const follower = followerRef.current;
    if (panLink === undefined || follower === null || box === null) return;
    panLink.publish(
      {
        x: centerFocus(scroller.scrollLeft, scroller.clientWidth, box.width),
        y: centerFocus(scroller.scrollTop, scroller.clientHeight, box.height),
      },
      follower,
    );
  };

  const onPointerDown = (event: PointerEvent<HTMLDivElement>) => {
    const scroller = scrollRef.current;
    if (scroller === null || event.button !== 0) return;
    event.currentTarget.setPointerCapture(event.pointerId);
    drag.current = {
      x: event.clientX,
      y: event.clientY,
      left: scroller.scrollLeft,
      top: scroller.scrollTop,
      moved: false,
    };
  };
  const onPointerMove = (event: PointerEvent<HTMLDivElement>) => {
    const scroller = scrollRef.current;
    const start = drag.current;
    if (scroller === null || start === null) return;
    const dx = event.clientX - start.x;
    const dy = event.clientY - start.y;
    if (Math.abs(dx) + Math.abs(dy) > DRAG_THRESHOLD) start.moved = true;
    scroller.scrollLeft = start.left - dx;
    scroller.scrollTop = start.top - dy;
    publishPan(scroller);
  };
  const onPointerUp = () => {
    const wasClick = drag.current !== null && !drag.current.moved;
    drag.current = null;
    if (wasClick) onClick();
  };

  return (
    <div
      ref={scrollRef}
      className={isVisible ? styles.fullLayer : styles.hiddenLayer}
      onPointerDown={onPointerDown}
      onPointerMove={onPointerMove}
      onPointerUp={onPointerUp}
      onPointerCancel={() => {
        drag.current = null;
      }}
    >
      <div className={styles.fullContent}>
        <div className={styles.rotatedBox} style={box ?? undefined}>
          <img
            className={styles.fullImage}
            src={originalUrl(item.id, item.imageVersion)}
            alt={item.fileName}
            draggable={false}
            style={{ ...size, transform: centeredRotation(item.rotation) }}
            onLoad={(event) => {
              const image = event.currentTarget;
              setSize(
                fullSizeInCssPixels(
                  image.naturalWidth,
                  image.naturalHeight,
                  window.devicePixelRatio,
                ),
              );
              onLoad();
            }}
            onError={onError}
          />
        </div>
      </div>
    </div>
  );
}

/** Message affiché par-dessus la photo (chargement, problème). */
function Status({
  message,
  isError,
}: {
  message: string | null;
  isError: boolean;
}) {
  // Le chargement n'est annoncé qu'au-delà d'un court délai : un aperçu déjà
  // prêt s'affiche sans clignotement.
  const [isShown, setIsShown] = useState(false);
  useEffect(() => {
    const timer = setTimeout(() => {
      setIsShown(true);
    }, 200);
    return () => {
      clearTimeout(timer);
      setIsShown(false);
    };
  }, [message]);

  if (message === null || (!isError && !isShown)) return null;
  return (
    <p className={styles.status} role={isError ? "alert" : "status"}>
      {message}
    </p>
  );
}

function problemMessage(
  problem: MediaFileStatus | "unknown" | null,
  t: TFunction,
): string | null {
  switch (problem) {
    case null:
      return null;
    case "missing":
      return t("loupe.missing");
    case "notInCatalog":
      return t("loupe.notInCatalog");
    case "available":
    case "unknown":
      return t("loupe.unreadable");
  }
}
