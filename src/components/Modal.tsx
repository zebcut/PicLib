import type { ReactNode } from "react";
import { useId } from "react";
import { createPortal } from "react-dom";

import styles from "./Modal.module.css";

/**
 * Fenêtre de message au-dessus de l'interface, qui bloque le reste de l'écran
 * jusqu'à ce que l'utilisateur choisisse une action. Elle est placée à la fin
 * du document (un « portail ») : ouverte depuis un panneau, elle n'est pas
 * recouverte par ce qui le suit (la grille).
 */
export function Modal({
  title,
  children,
  actions,
}: {
  title: string;
  children: ReactNode;
  /** Boutons, du moins au plus important (le dernier est à droite). */
  actions: ReactNode;
}) {
  const titleId = useId();

  return createPortal(
    <div className={styles.overlay}>
      <div
        className={styles.panel}
        role="dialog"
        aria-modal="true"
        aria-labelledby={titleId}
      >
        <h2 id={titleId} className={styles.title}>
          {title}
        </h2>
        <div className={styles.body}>{children}</div>
        <div className={styles.actions}>{actions}</div>
      </div>
    </div>,
    document.body,
  );
}
