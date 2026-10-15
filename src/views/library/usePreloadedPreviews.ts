import { useEffect, useRef } from "react";

import { previewUrl } from "../../services/library";

/**
 * Demande à l'avance les aperçus des médias indiqués (identifiant et
 * version de l'image) : le navigateur les garde en mémoire, et le cœur les
 * fabrique s'ils n'existent pas encore.
 */
export function usePreloadedPreviews(
  items: readonly { id: number; imageVersion: number }[],
): void {
  const key = items
    .map((item) => `${String(item.id)}:${String(item.imageVersion)}`)
    .join(",");
  // Garde les images en cours de chargement, pour qu'elles ne soient pas
  // abandonnées avant la fin.
  const loading = useRef<HTMLImageElement[]>([]);

  useEffect(() => {
    loading.current = key
      .split(",")
      .filter((entry) => entry !== "")
      .map((entry) => {
        const [id, version] = entry.split(":");
        const image = new Image();
        image.src = previewUrl(Number(id), Number(version));
        return image;
      });
  }, [key]);
}
