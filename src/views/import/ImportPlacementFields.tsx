import { useTranslation } from "react-i18next";

import type {
  DestinationChoice,
  ImportModeDto,
} from "../../services/importDestination";
import { Button } from "../../components/Button";
import styles from "../../components/jobs/JobDialogs.module.css";

/** Choix de la fenêtre d'import : laisser les fichiers, ou les copier. */
export interface PlacementChoice {
  mode: ImportModeDto;
  /** Dossier où copier, désigné sans chemin. */
  destination: DestinationChoice;
  /** Ce dossier, tel qu'il est affiché. */
  shownDestination: string;
  /** Ajouter le chemin d'origine de chaque fichier à sa légende. */
  originalPathInCaption: boolean;
}

/**
 * Laisser les fichiers à leur place ou les copier, et les
 * réglages de la copie : dossier de destination, chemin d'origine dans la
 * légende. Une carte mémoire ne se copie que.
 */
export function ImportPlacementFields({
  choice,
  defaultDestination,
  isCameraCard,
  canCopy,
  chooseFailed,
  onChange,
  onChooseDestination,
}: {
  choice: PlacementChoice;
  /** Dossier `Media` du catalogue, affiché. */
  defaultDestination: string;
  isCameraCard: boolean;
  /** Faux si les réglages de la copie n'ont pas pu être lus. */
  canCopy: boolean;
  /** Vrai si le dernier « Choisir… » a échoué. */
  chooseFailed: boolean;
  onChange: (choice: PlacementChoice) => void;
  onChooseDestination: () => void;
}) {
  const { t } = useTranslation();
  const isCopy = choice.mode === "copy";

  return (
    <>
      <fieldset className={styles.choices}>
        <legend className={styles.legend}>
          {t("folderAnalysis.placement")}
        </legend>
        <label className={styles.option}>
          <input
            type="radio"
            name="placement"
            checked={!isCopy}
            disabled={isCameraCard}
            onChange={() => {
              onChange({ ...choice, mode: "reference" });
            }}
          />
          {t("folderAnalysis.leaveInPlace")}
        </label>
        <label className={styles.option}>
          <input
            type="radio"
            name="placement"
            checked={isCopy}
            disabled={!canCopy}
            onChange={() => {
              onChange({ ...choice, mode: "copy" });
            }}
          />
          {t("folderAnalysis.copyByDate")}
        </label>
      </fieldset>
      {isCameraCard && (
        <p className={styles.secondary}>
          {t("folderAnalysis.cameraCardCopyOnly")}
        </p>
      )}
      {isCopy ? (
        <>
          <div className={styles.destination}>
            <span>{t("folderAnalysis.destination")}</span>
            <span className={styles.path}>{choice.shownDestination}</span>
            <Button onClick={onChooseDestination}>
              {t("folderAnalysis.chooseDestination")}
            </Button>
            {choice.shownDestination !== defaultDestination && (
              <Button
                onClick={() => {
                  onChange({
                    ...choice,
                    destination: "default",
                    shownDestination: defaultDestination,
                  });
                }}
              >
                {t("folderAnalysis.defaultDestination")}
              </Button>
            )}
          </div>
          {chooseFailed && (
            <p className={styles.warning}>
              {t("folderAnalysis.destinationUnsupported")}
            </p>
          )}
          <label className={styles.option}>
            <input
              type="checkbox"
              checked={choice.originalPathInCaption}
              onChange={(event) => {
                onChange({
                  ...choice,
                  originalPathInCaption: event.target.checked,
                });
              }}
            />
            {t("folderAnalysis.originalPathInCaption")}
          </label>
          <p className={styles.secondary}>
            {t("folderAnalysis.copyExplanation")}
          </p>
        </>
      ) : (
        <p className={styles.secondary}>
          {t("folderAnalysis.referenceExplanation")}
        </p>
      )}
    </>
  );
}
