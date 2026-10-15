import { useId } from "react";
import type { ReactNode } from "react";

import { useStoredBoolean } from "../state/useStoredBoolean";
import { DisclosureTriangle } from "./DisclosureTriangle";
import styles from "./PanelSection.module.css";

/**
 * Section repliable d'une colonne latérale, comme les panneaux
 * de Lightroom : un clic sur le titre la replie ou la déplie, et cet état
 * est retenu d'une session à l'autre.
 *
 * @param storageKey clé sous laquelle l'état replié est retenu.
 * @param actions boutons ajoutés à droite du titre (« + » d'un panneau).
 * @param className classe ajoutée à la section, `bodyClassName` à son
 *   contenu (une section qui prend toute la hauteur restante, par exemple).
 */
export function PanelSection({
  title,
  storageKey,
  className,
  bodyClassName,
  actions,
  children,
}: {
  title: string;
  storageKey: string;
  className?: string;
  bodyClassName?: string;
  actions?: ReactNode;
  children: ReactNode;
}) {
  const [isOpen, setIsOpen] = useStoredBoolean(storageKey, true);
  const bodyId = useId();

  return (
    <section
      className={[styles.section, className].join(" ").trim()}
      aria-label={title}
    >
      <h2 className={styles.title}>
        <button
          type="button"
          className={styles.header}
          aria-expanded={isOpen}
          aria-controls={bodyId}
          onClick={() => {
            setIsOpen(!isOpen);
          }}
        >
          <DisclosureTriangle expanded={isOpen} />
          {title}
        </button>
        {actions !== undefined && (
          <span className={styles.actions}>{actions}</span>
        )}
      </h2>
      {isOpen && (
        <div
          id={bodyId}
          className={[styles.body, bodyClassName].join(" ").trim()}
        >
          {children}
        </div>
      )}
    </section>
  );
}
