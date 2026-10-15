import { useEffect, useRef } from "react";
import type { KeyboardEvent, PointerEvent as ReactPointerEvent } from "react";

import styles from "./ResizeHandle.module.css";

/** Pas d'une flèche du clavier, en pixels (Maj : dix fois plus). */
const KEY_STEP_PX = 10;

/**
 * Poignée qui règle une largeur ou une hauteur en la glissant :
 * le bord d'une colonne latérale, le bas de la liste des mots-clés. Au
 * clavier, les flèches la déplacent, Début et Fin vont aux limites ; un
 * double-clic revient à la taille par défaut.
 *
 * @param orientation `vertical` : un trait vertical qu'on glisse à gauche
 *   ou à droite (une largeur) ; `horizontal` : un trait qu'on glisse en haut
 *   ou en bas (une hauteur).
 * @param direction `1` si glisser vers la droite (ou le bas) agrandit, `-1`
 *   si cela rétrécit (bord gauche de la colonne de droite).
 */
export function ResizeHandle({
  label,
  orientation,
  direction,
  value,
  min,
  max,
  defaultValue,
  onChange,
}: {
  label: string;
  orientation: "vertical" | "horizontal";
  direction: 1 | -1;
  value: number;
  min: number;
  max: number;
  defaultValue: number;
  onChange: (value: number) => void;
}) {
  const stop = useRef<(() => void) | null>(null);
  const isVertical = orientation === "vertical";

  // Un glisser interrompu par la disparition du composant.
  useEffect(
    () => () => {
      stop.current?.();
    },
    [],
  );

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    event.preventDefault();
    stop.current?.();
    const origin = isVertical ? event.clientX : event.clientY;
    const start = value;
    const onMove = (move: PointerEvent) => {
      const position = isVertical ? move.clientX : move.clientY;
      onChange(start + (position - origin) * direction);
    };
    const finish = () => {
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", finish);
      window.removeEventListener("pointercancel", finish);
      document.body.classList.remove(
        (isVertical ? styles.resizingColumns : styles.resizingRows) ?? "",
      );
      stop.current = null;
    };
    stop.current = finish;
    // Le curseur reste celui du redimensionnement tant qu'on glisse, même
    // hors de la poignée.
    document.body.classList.add(
      (isVertical ? styles.resizingColumns : styles.resizingRows) ?? "",
    );
    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", finish);
    window.addEventListener("pointercancel", finish);
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    const step = event.shiftKey ? KEY_STEP_PX * 10 : KEY_STEP_PX;
    // Sens à l'écran : droite ou bas, positif.
    const moves: Record<string, number> = isVertical
      ? { ArrowLeft: -step, ArrowRight: step }
      : { ArrowUp: -step, ArrowDown: step };
    const move = moves[event.key];
    if (move !== undefined) onChange(value + move * direction);
    else if (event.key === "Home") onChange(min);
    else if (event.key === "End") onChange(max);
    else return;
    event.preventDefault();
  };

  return (
    <div
      className={isVertical ? styles.vertical : styles.horizontal}
      role="separator"
      aria-orientation={orientation}
      aria-label={label}
      title={label}
      aria-valuenow={Math.round(value)}
      aria-valuemin={min}
      aria-valuemax={max}
      tabIndex={0}
      onPointerDown={onPointerDown}
      onKeyDown={onKeyDown}
      onDoubleClick={() => {
        onChange(defaultValue);
      }}
    />
  );
}
