import type { TFunction } from "i18next";
import { useId, useState } from "react";
import { useTranslation } from "react-i18next";

import type { KeywordRefusal } from "../../state/useCatalogEdits";
import { KeywordCombobox } from "./KeywordCombobox";
import styles from "./KeywordPanel.module.css";

/**
 * Champ de saisie des mots-clés : `Lieux > France > Paris,
 * Vacances`, avec autocomplétion du mot-clé en cours (ce qui suit la
 * dernière virgule). Entrée sans suggestion choisie ajoute la saisie.
 *
 * @param onSubmit ajoute la saisie aux photos sélectionnées ; renvoie la
 *   raison d'un refus, ou `null` (le champ est alors vidé).
 */
export function KeywordEntry({
  onSubmit,
}: {
  onSubmit: (entry: string) => Promise<KeywordRefusal | null>;
}) {
  const { t } = useTranslation();
  const errorId = useId();
  const [text, setText] = useState("");
  const [refusal, setRefusal] = useState<KeywordRefusal | null>(null);

  const submit = () => {
    if (text.trim() === "") return;
    const entry = text;
    onSubmit(entry)
      .then((result) => {
        setRefusal(result);
        // Le texte n'est effacé que s'il n'a pas changé entre-temps.
        if (result === null)
          setText((current) => (current === entry ? "" : current));
      })
      .catch((error: unknown) => {
        console.error("Failed to add keywords", error);
      });
  };

  return (
    <div>
      <KeywordCombobox
        multiple
        value={text}
        onChange={(next) => {
          setText(next);
          setRefusal(null);
        }}
        onEnter={submit}
        label={t("keywords.entryLabel")}
        placeholder={t("keywords.entryPlaceholder")}
        title={t("keywords.entryHint")}
        describedBy={refusal === null ? undefined : errorId}
      />
      {refusal !== null && (
        <p id={errorId} className={styles.error} role="alert">
          {refusalText(t, refusal)}
        </p>
      )}
    </div>
  );
}

function refusalText(t: TFunction, refusal: KeywordRefusal): string {
  switch (refusal.kind) {
    case "ambiguous":
      return t("keywords.ambiguous", { name: refusal.name });
    case "tooLong":
      return t("keywords.tooLong");
    case "forbiddenCharacter":
      return t("keywords.forbidden", { name: refusal.name });
  }
}
