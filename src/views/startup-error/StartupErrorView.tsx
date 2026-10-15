import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import type { StartupFailure } from "../../services/startup";
import styles from "./StartupErrorView.module.css";

/**
 * Échec à afficher : un échec signalé par le cœur Rust, ou l'absence de
 * réponse du cœur lui-même.
 */
export type DisplayedFailure = StartupFailure | { kind: "coreUnreachable" };

/** Actions proposées pour sortir de l'erreur en changeant de catalogue. */
export interface CatalogActions {
  openCatalog: () => void;
  createCatalog: () => void;
}

/**
 * Écran affiché quand le catalogue n'a pas pu être ouvert au démarrage.
 *
 * @param catalogActions si fournies, boutons pour ouvrir un autre catalogue
 *   ou en créer un nouveau.
 */
export function StartupErrorView({
  failure,
  catalogActions,
}: {
  failure: DisplayedFailure;
  catalogActions?: CatalogActions;
}) {
  const { t } = useTranslation();
  const location = failureLocation(failure);

  return (
    <main className={styles.container} role="alert">
      <h1 className={styles.title}>{t("startupError.title")}</h1>
      <p className={styles.message}>{failureMessage(t, failure)}</p>
      {location !== null && (
        <p className={styles.location}>
          {location.isBackup
            ? t("startupError.backupLabel", { path: location.path })
            : t("startupError.pathLabel", { path: location.path })}
        </p>
      )}
      {catalogActions !== undefined && (
        <div className={styles.actions}>
          <Button onClick={catalogActions.createCatalog}>
            {t("startupError.createCatalog")}
          </Button>
          <Button variant="primary" onClick={catalogActions.openCatalog}>
            {t("startupError.openCatalog")}
          </Button>
        </div>
      )}
      <p className={styles.hint}>{t("startupError.logHint")}</p>
    </main>
  );
}

/** Message traduit expliquant l'échec. */
function failureMessage(t: TFunction, failure: DisplayedFailure): string {
  switch (failure.kind) {
    case "coreUnreachable":
      return t("startupError.coreUnreachable");
    case "systemDirectories":
      return t("startupError.systemDirectories");
    case "settingsUnreadable":
      return t("startupError.settingsUnreadable");
    case "workspaceUnavailable":
      return t("startupError.workspaceUnavailable");
    case "workspaceCreationFailed":
      return t("startupError.workspaceCreationFailed");
    case "catalogMissing":
      return t("startupError.catalogMissing");
    case "catalogUnreadable":
      return t("startupError.catalogUnreadable");
    case "notAPicLibCatalog":
      return t("startupError.notAPicLibCatalog");
    case "catalogTooNew":
      return t("startupError.catalogTooNew", {
        found: failure.found,
        supported: failure.supported,
      });
    case "backupFailed":
      return t("startupError.backupFailed");
    case "migrationFailed":
      return t("startupError.migrationFailed");
  }
}

/** Fichier ou dossier concerné par l'échec, s'il y en a un à montrer. */
function failureLocation(
  failure: DisplayedFailure,
): { path: string; isBackup: boolean } | null {
  if (failure.kind === "migrationFailed") {
    return failure.backup === null
      ? null
      : { path: failure.backup, isBackup: true };
  }
  return "path" in failure ? { path: failure.path, isBackup: false } : null;
}
