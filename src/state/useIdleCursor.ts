import { useEffect, useState } from "react";

/**
 * Vrai quand la souris n'a pas bougé depuis `delayMs` millisecondes : le
 * pointeur peut alors être masqué pour ne pas gêner la vue.
 */
export function useIdleCursor(delayMs: number): boolean {
  const [isIdle, setIsIdle] = useState(false);

  useEffect(() => {
    const becomeIdle = () => {
      setIsIdle(true);
    };
    let timer = setTimeout(becomeIdle, delayMs);
    const onMove = () => {
      setIsIdle(false);
      clearTimeout(timer);
      timer = setTimeout(becomeIdle, delayMs);
    };
    window.addEventListener("mousemove", onMove);
    return () => {
      clearTimeout(timer);
      window.removeEventListener("mousemove", onMove);
    };
  }, [delayMs]);

  return isIdle;
}
