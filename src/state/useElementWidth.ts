import { useEffect, useState } from "react";
import type { RefObject } from "react";

/**
 * Largeur intérieure d'un élément (sans sa barre de défilement), tenue à jour
 * quand la fenêtre ou le panneau change de taille. Vaut 0 avant la première
 * mesure.
 */
export function useElementWidth(ref: RefObject<HTMLElement | null>): number {
  const [width, setWidth] = useState(0);

  useEffect(() => {
    const element = ref.current;
    if (element === null) return;
    const observer = new ResizeObserver(() => {
      setWidth(element.clientWidth);
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [ref]);

  return width;
}
