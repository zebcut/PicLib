import { useEffect, useState } from "react";

/** État d'une donnée demandée au cœur Rust. */
export type BackendData<T> =
  { status: "loading" } | { status: "ready"; data: T } | { status: "error" };

/**
 * Charge une donnée auprès du cœur Rust au premier affichage du composant.
 *
 * @param load fonction d'appel au cœur ; elle doit être stable (définie au
 *   niveau d'un module), sinon l'appel serait répété à chaque affichage.
 * @param description nom de la donnée, pour la console en cas d'erreur.
 * @param refreshKey la donnée est rechargée chaque fois que cette valeur
 *   change (par exemple après un import).
 */
export function useBackendData<T>(
  load: () => Promise<T>,
  description: string,
  refreshKey = 0,
): BackendData<T> {
  const [state, setState] = useState<BackendData<T>>({ status: "loading" });

  useEffect(() => {
    // Évite de mettre à jour un composant déjà retiré de l'écran si la
    // réponse arrive après coup.
    let isActive = true;
    load().then(
      (data) => {
        if (isActive) setState({ status: "ready", data });
      },
      (error: unknown) => {
        console.error(`Failed to load ${description}`, error);
        if (isActive) setState({ status: "error" });
      },
    );
    return () => {
      isActive = false;
    };
  }, [load, description, refreshKey]);

  return state;
}
