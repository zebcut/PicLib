import { useTranslation } from "react-i18next";

import { fetchAppInfo } from "../../services/appInfo";
import type { CatalogOverview } from "../../services/catalogInfo";
import type { AppInfo } from "../../services/appInfo";
import { useBackendData } from "../../state/useBackendData";
import type { BackendData } from "../../state/useBackendData";
import styles from "./WelcomeView.module.css";

/**
 * Écran d'accueil affiché quand le catalogue est vide (ou que son contenu
 * n'a pas pu être lu).
 *
 * @param overview contenu du catalogue.
 * @param onImport lance le choix d'un dossier de photos à importer.
 */
export function WelcomeView({
  workspaceDir,
  overview,
  onImport,
}: {
  workspaceDir: string;
  overview: BackendData<CatalogOverview>;
  onImport: () => void;
}) {
  const { t } = useTranslation();
  const appInfo = useBackendData(fetchAppInfo, "application info");

  return (
    <main className={styles.container}>
      <h1 className={styles.title}>{t("welcome.title")}</h1>
      <p className={styles.subtitle}>{t("welcome.subtitle")}</p>
      <CatalogContents overview={overview} />
      <button
        type="button"
        className={styles.importButton}
        title={t("welcome.importTooltip")}
        onClick={onImport}
      >
        {t("welcome.importButton")}
      </button>
      <div className={styles.footer}>
        <p className={styles.workspace}>
          {t("welcome.workspace", { path: workspaceDir })}
        </p>
        <VersionLine appInfo={appInfo} />
      </div>
    </main>
  );
}

/** « Catalogue vide », ou l'échec de sa lecture ; rien tant qu'il est lu. */
function CatalogContents({
  overview,
}: {
  overview: BackendData<CatalogOverview>;
}) {
  const { t } = useTranslation();
  switch (overview.status) {
    case "loading":
      return null;
    case "ready":
      return <p className={styles.contents}>{t("welcome.catalogEmpty")}</p>;
    case "error":
      return (
        <p className={styles.contents} role="alert">
          {t("welcome.catalogUnreadable")}
        </p>
      );
  }
}

function VersionLine({ appInfo }: { appInfo: BackendData<AppInfo> }) {
  const { t } = useTranslation();

  switch (appInfo.status) {
    case "loading":
      return null;
    case "ready":
      return (
        <p className={styles.version}>
          {t("welcome.version", { version: appInfo.data.version })}
        </p>
      );
    case "error":
      return (
        <p className={styles.version} role="alert">
          {t("welcome.versionUnavailable")}
        </p>
      );
  }
}
