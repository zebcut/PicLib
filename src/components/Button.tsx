import type { ButtonHTMLAttributes } from "react";

import styles from "./Button.module.css";

/** Bouton de l'application ; `primary` met en avant l'action conseillée. */
export function Button({
  variant = "secondary",
  className,
  ...props
}: ButtonHTMLAttributes<HTMLButtonElement> & {
  variant?: "primary" | "secondary";
}) {
  const classes = [styles.button, styles[variant], className]
    .filter(Boolean)
    .join(" ");
  return <button type="button" className={classes} {...props} />;
}
