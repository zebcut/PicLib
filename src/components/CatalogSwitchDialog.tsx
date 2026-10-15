import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import type {
  CatalogSwitch,
  CatalogSwitchFailure,
} from "../state/useCatalogSwitch";
import { Button } from "./Button";
import styles from "./CatalogSwitchDialog.module.css";
import { Modal } from "./Modal";

/**
 * Messages du changement de catalogue : avertissement « emplacement
 * synchronisé », catalogue déjà ouvert, ou échec.
 */
export function CatalogSwitchDialog({
  catalogSwitch,
}: {
  catalogSwitch: CatalogSwitch;
}) {
  const { t } = useTranslation();
  const { state, confirmCreation, dismiss } = catalogSwitch;

  switch (state.step) {
    case "idle":
    case "busy":
      return null;

    case "alreadyOpen":
      return (
        <Modal
          title={t("catalogSwitch.alreadyOpenTitle")}
          actions={<OkButton onClick={dismiss} />}
        >
          <p className={styles.text}>{t("catalogSwitch.alreadyOpen")}</p>
        </Modal>
      );

    case "syncedLocation":
      return (
        <Modal
          title={t("catalogSwitch.syncedTitle")}
          actions={
            <>
              <Button onClick={dismiss}>{t("common.cancel")}</Button>
              <Button
                onClick={() => {
                  confirmCreation(false);
                }}
              >
                {t("catalogSwitch.keepName")}
              </Button>
              <Button
                variant="primary"
                autoFocus
                onClick={() => {
                  confirmCreation(true);
                }}
              >
                {t("catalogSwitch.addNosync")}
              </Button>
            </>
          }
        >
          <p className={styles.text}>{t("catalogSwitch.syncedMessage")}</p>
          <p className={styles.path}>
            {t("catalogSwitch.chosenLocation", { path: state.path })}
          </p>
          <p className={styles.path}>
            {t("catalogSwitch.suggestedLocation", {
              path: state.suggestedPath,
            })}
          </p>
        </Modal>
      );

    case "failed": {
      const location = failureLocation(state.failure);
      return (
        <Modal
          title={t("catalogSwitch.failedTitle")}
          actions={<OkButton onClick={dismiss} />}
        >
          <p className={styles.text}>{failureMessage(t, state.failure)}</p>
          {location !== null && (
            <p className={styles.path}>
              {t("catalogSwitch.location", { path: location })}
            </p>
          )}
        </Modal>
      );
    }
  }
}

function OkButton({ onClick }: { onClick: () => void }) {
  const { t } = useTranslation();
  return (
    <Button variant="primary" autoFocus onClick={onClick}>
      {t("common.ok")}
    </Button>
  );
}

/** Message traduit expliquant pourquoi le catalogue n'a pas changé. */
function failureMessage(t: TFunction, failure: CatalogSwitchFailure): string {
  switch (failure.kind) {
    case "coreUnreachable":
      return t("catalogSwitch.failure.coreUnreachable");
    case "systemDirectories":
      return t("catalogSwitch.failure.systemDirectories");
    case "unsupportedLocation":
      return t("catalogSwitch.failure.unsupportedLocation");
    case "notAPicLibFolder":
      return t("catalogSwitch.failure.notAPicLibFolder");
    case "notAPicLibCatalog":
      return t("catalogSwitch.failure.notAPicLibCatalog");
    case "catalogUnreadable":
      return t("catalogSwitch.failure.catalogUnreadable");
    case "catalogTooNew":
      return t("catalogSwitch.failure.catalogTooNew", {
        found: failure.found,
        supported: failure.supported,
      });
    case "alreadyExists":
      return failure.containsCatalog
        ? t("catalogSwitch.failure.alreadyExistsWithCatalog")
        : t("catalogSwitch.failure.alreadyExists");
    case "creationFailed":
      return t("catalogSwitch.failure.creationFailed");
    case "settingsNotSaved":
      return t("catalogSwitch.failure.settingsNotSaved");
    case "noPendingCreation":
      return t("catalogSwitch.failure.noPendingCreation");
  }
}

/** Chemin concerné par l'échec, s'il y en a un. */
function failureLocation(failure: CatalogSwitchFailure): string | null {
  return "path" in failure ? failure.path : null;
}
