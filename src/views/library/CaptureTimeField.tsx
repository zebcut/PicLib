import { useId, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { counted, formatCaptureTime } from "../../i18n/format";
import type { CaptureTimeState } from "../../services/captureTime";
import styles from "./MetadataPanel.module.css";

/**
 * Date de prise de vue des photos sélectionnées, modifiable
 * comme les autres champs du panneau : la date saisie est donnée telle
 * quelle à toutes, sans aucun calcul. Entrée (ou quitter le champ) valide,
 * Échap abandonne. En dessous, la date d'origine de la photo active quand
 * elle a changé, et un bouton pour rendre la leur aux photos modifiées.
 *
 * @param originalTime date d'origine de la photo active, si sa date a été
 *   modifiée.
 * @param onSet donne la date saisie (`2024-07-14T18:30:05`) à la sélection.
 * @param onRestore rend leur date d'origine aux photos sélectionnées.
 */
export function CaptureTimeField({
  state,
  originalTime,
  onSet,
  onRestore,
}: {
  state: CaptureTimeState;
  originalTime: string | null;
  onSet: (local: string) => void;
  onRestore: () => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const current = state.value.kind === "same" ? (state.value.value ?? "") : "";
  // `null` tant que rien n'a été saisi.
  const [draft, setDraft] = useState<string | null>(null);
  // Échap quitte le champ sans valider : sa sortie ne doit rien enregistrer.
  const isCancelling = useRef(false);

  const commit = () => {
    if (isCancelling.current) {
      isCancelling.current = false;
      return;
    }
    // Une date ne peut pas être vide : un champ effacé ne change rien.
    if (draft !== null && draft !== "" && draft !== current) onSet(draft);
  };
  const onKeyDown = (event: KeyboardEvent<HTMLInputElement>) => {
    if (event.key === "Escape") {
      event.preventDefault();
      isCancelling.current = true;
      setDraft(null);
      event.currentTarget.blur();
    } else if (event.key === "Enter") {
      event.preventDefault();
      event.currentTarget.blur();
    }
  };

  return (
    <div className={styles.field}>
      <label className={styles.label} htmlFor={id}>
        {t("metadata.captureTime")}
        {state.value.kind === "mixed" && (
          <span className={styles.mixed}> {t("metadata.mixed")}</span>
        )}
      </label>
      <input
        id={id}
        className={styles.input}
        type="datetime-local"
        step={1}
        min="0001-01-01T00:00:00"
        max="9999-12-31T23:59:59"
        title={t("metadata.fieldHint")}
        value={draft ?? current}
        onChange={(event) => {
          setDraft(event.target.value);
        }}
        onBlur={commit}
        onKeyDown={onKeyDown}
      />
      {originalTime !== null && (
        <p className={styles.note}>
          {t("captureTime.original", {
            date: formatCaptureTime(originalTime),
          })}
        </p>
      )}
      {state.modifiedCount > 0 && (
        <Button
          title={t("captureTime.restoreHint", counted(state.modifiedCount))}
          onClick={onRestore}
        >
          {t("captureTime.restore")}
        </Button>
      )}
    </div>
  );
}
