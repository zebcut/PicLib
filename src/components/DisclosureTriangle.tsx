import styles from "./DisclosureTriangle.module.css";

/**
 * Triangle d'une section ou d'une branche repliable : il pointe à droite
 * quand elle est repliée, vers le bas quand elle est dépliée. Dessiné (et
 * non tapé comme « ▸ ») pour rester net et assez grand à l'écran.
 */
export function DisclosureTriangle({ expanded }: { expanded: boolean }) {
  return (
    <svg
      className={[styles.triangle, expanded ? styles.expanded : undefined]
        .join(" ")
        .trim()}
      viewBox="0 0 10 10"
      aria-hidden
    >
      <path d="M3 1.5 L8 5 L3 8.5 Z" />
    </svg>
  );
}
