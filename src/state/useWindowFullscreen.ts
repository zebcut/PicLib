import { useEffect, useRef } from "react";

import { isWindowFullscreen, setWindowFullscreen } from "../services/appWindow";

/**
 * Passe la fenêtre en plein écran tant que le composant est affiché, puis la
 * remet comme elle était : si elle était déjà en plein écran, elle y reste.
 *
 * @param onLeft appelé si l'utilisateur quitte lui-même le plein écran
 *   (bouton vert, Ctrl+Cmd+F).
 */
export function useWindowFullscreen(onLeft: () => void): void {
  const latestOnLeft = useRef(onLeft);
  useEffect(() => {
    latestOnLeft.current = onLeft;
  });

  useEffect(() => {
    let isActive = true;
    // Tant que l'état de départ n'est pas connu, rien ne sera défait.
    let wasFullscreen = true;
    // Le plein écran n'est constaté qu'à la fin de l'animation d'entrée :
    // avant, une fenêtre pas encore en plein écran ne veut rien dire.
    let isConfirmed = false;

    isWindowFullscreen()
      .then(async (fullscreen) => {
        if (!isActive) return;
        wasFullscreen = fullscreen;
        isConfirmed = fullscreen;
        if (!fullscreen) await setWindowFullscreen(true);
      })
      .catch(logFailure);

    // Entrer ou sortir du plein écran change la taille de la page.
    const onResize = () => {
      isWindowFullscreen().then((fullscreen) => {
        if (!isActive) return;
        if (fullscreen) isConfirmed = true;
        else if (isConfirmed) latestOnLeft.current();
      }, logFailure);
    };
    window.addEventListener("resize", onResize);

    return () => {
      isActive = false;
      window.removeEventListener("resize", onResize);
      if (!wasFullscreen) setWindowFullscreen(false).catch(logFailure);
    };
  }, []);
}

function logFailure(error: unknown): void {
  console.error("Failed to change the full screen state", error);
}
