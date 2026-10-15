import { getCurrentWindow } from "@tauri-apps/api/window";

/**
 * Indique si la fenêtre de PicLib est en plein écran.
 *
 * @throws si Tauri refuse l'appel.
 */
export function isWindowFullscreen(): Promise<boolean> {
  return getCurrentWindow().isFullscreen();
}

/**
 * Passe la fenêtre de PicLib en plein écran (celui du système, dans son
 * propre espace sur macOS) ou l'en fait sortir.
 *
 * @throws si Tauri refuse l'appel.
 */
export function setWindowFullscreen(fullscreen: boolean): Promise<void> {
  return getCurrentWindow().setFullscreen(fullscreen);
}
