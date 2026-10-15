import { useEffect, useState } from "react";
import type { RefObject } from "react";

/**
 * Taille intérieure d'un élément, tenue à jour quand la fenêtre ou le
 * panneau change de taille. Vaut 0 × 0 avant la première mesure.
 */
export function useElementSize(ref: RefObject<HTMLElement | null>): {
  width: number;
  height: number;
} {
  const [size, setSize] = useState({ width: 0, height: 0 });

  useEffect(() => {
    const element = ref.current;
    if (element === null) return;
    const observer = new ResizeObserver(() => {
      setSize({ width: element.clientWidth, height: element.clientHeight });
    });
    observer.observe(element);
    return () => {
      observer.disconnect();
    };
  }, [ref]);

  return size;
}
