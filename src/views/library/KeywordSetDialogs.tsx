import type { TFunction } from "i18next";
import { useId, useMemo, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import type { KeywordListItemDto } from "../../services/keywordList";
import type {
  KeywordSetDto,
  KeywordSetOperation,
  KeywordSetRefusal,
  KeywordSetSlotInput,
  KeywordSetSlotRefusal,
} from "../../services/keywordSets";
import {
  DEFAULT_BINDINGS,
  isApplePlatform,
  KEYWORD_SET_SLOTS,
  keywordSetShortcut,
  shortcutLabel,
} from "../../shortcuts/registry";
import { itemPath } from "../../utils/tree";
import { NameDialog } from "./KeywordDialogs";
import { KeywordCombobox } from "./KeywordCombobox";
import shared from "./KeywordSection.module.css";
import styles from "./KeywordSetSection.module.css";

/** Fenêtre ouverte depuis la section « Jeu de mots-clés ». */
export type KeywordSetDialogState =
  | { kind: "create" }
  | { kind: "rename"; set: KeywordSetDto }
  | { kind: "edit"; set: KeywordSetDto; slot: number }
  | { kind: "delete"; set: KeywordSetDto };

/**
 * Fenêtre des jeux de mots-clés : nom (création, renommage), cases (modifier
 * le jeu), confirmation de suppression.
 *
 * @param onRun modifie les jeux ; renvoie la raison d'un refus, ou `null`
 *   (la fenêtre est alors fermée).
 */
export function KeywordSetDialog({
  dialog,
  keywords,
  onRun,
  onClose,
}: {
  dialog: KeywordSetDialogState;
  keywords: readonly KeywordListItemDto[];
  onRun: (operation: KeywordSetOperation) => Promise<KeywordSetRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const refusalText = (refusal: KeywordSetRefusal) =>
    setRefusalText(t, refusal);
  switch (dialog.kind) {
    case "create":
      return (
        <NameDialog
          title={t("keywordSets.createTitle")}
          initialName=""
          askExport={false}
          onSubmit={(name) => onRun({ kind: "create", name })}
          refusalText={refusalText}
          onClose={onClose}
        />
      );
    case "rename":
      return (
        <NameDialog
          title={t("keywordSets.renameTitle", { name: dialog.set.name })}
          initialName={dialog.set.name}
          askExport={false}
          onSubmit={(name) =>
            onRun({ kind: "rename", setId: dialog.set.id, name })
          }
          refusalText={refusalText}
          onClose={onClose}
        />
      );
    case "edit":
      return (
        <EditSetDialog
          set={dialog.set}
          keywords={keywords}
          focusSlot={dialog.slot}
          onSave={(slots) =>
            onRun({ kind: "fill", setId: dialog.set.id, slots })
          }
          onClose={onClose}
        />
      );
    case "delete":
      return (
        <DeleteSetDialog
          set={dialog.set}
          onDelete={() => onRun({ kind: "delete", setId: dialog.set.id })}
          onClose={onClose}
        />
      );
  }
}

/**
 * Les neuf cases d'un jeu, chacune avec l'autocomplétion de la saisie. Un
 * mot-clé choisi dans les suggestions est désigné exactement ; un chemin
 * tapé est lu comme à la saisie, et créé s'il n'existe pas.
 */
function EditSetDialog({
  set,
  keywords,
  focusSlot,
  onSave,
  onClose,
}: {
  set: KeywordSetDto;
  keywords: readonly KeywordListItemDto[];
  /** Case qui reçoit le focus, de 1 à 9. */
  focusSlot: number;
  onSave: (slots: KeywordSetSlotInput[]) => Promise<KeywordSetRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const errorId = useId();
  const initial = useMemo(() => {
    const byId = new Map(keywords.map((keyword) => [keyword.id, keyword]));
    return KEYWORD_SET_SLOTS.map((_, index): KeywordSetSlotInput => {
      const keywordId = set.slots[index] ?? null;
      return {
        keywordId,
        text: keywordId === null ? "" : itemPath(byId, keywordId),
      };
    });
  }, [set, keywords]);
  const [slots, setSlots] = useState(initial);
  const [refusal, setRefusal] = useState<KeywordSetRefusal | null>(null);
  const [isSaving, setIsSaving] = useState(false);
  const [failed, setFailed] = useState(false);
  const isApple = isApplePlatform();

  const save = () => {
    if (isSaving) return;
    setIsSaving(true);
    onSave(slots)
      .then((result) => {
        setRefusal(result);
        setIsSaving(false);
      })
      .catch((error: unknown) => {
        console.error("Failed to save the keyword set", error);
        setFailed(true);
        setIsSaving(false);
      });
  };
  const refusedSlot = refusal?.kind === "slot" ? refusal.slot : null;

  return (
    <Modal
      title={t("keywordSets.editTitle", { name: set.name })}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button variant="primary" disabled={isSaving} onClick={save}>
            {t("keywordSets.save")}
          </Button>
        </>
      }
    >
      <div
        className={styles.form}
        onKeyDown={(event) => {
          // Échap ferme la fenêtre, sauf s'il ferme d'abord une liste de
          // suggestions (qui arrête alors l'événement).
          if (event.key === "Escape") onClose();
        }}
      >
        <p className={shared.hint}>{t("keywordSets.editHint")}</p>
        <div className={styles.fields}>
          {KEYWORD_SET_SLOTS.map((slot, index) => {
            const binding = DEFAULT_BINDINGS[keywordSetShortcut(slot)];
            const shortcut =
              binding === null ? "" : shortcutLabel(binding, isApple);
            const isRefused = refusedSlot === slot;
            return (
              <div key={slot} className={styles.field}>
                <span className={styles.fieldLabel} aria-hidden>
                  {shortcut}
                </span>
                <KeywordCombobox
                  multiple={false}
                  value={slots[index]?.text ?? ""}
                  onChange={(text, picked) => {
                    setSlots((current) =>
                      current.map((value, other) =>
                        other === index
                          ? { text, keywordId: picked?.id ?? null }
                          : value,
                      ),
                    );
                    if (isRefused) setRefusal(null);
                  }}
                  onEnter={save}
                  label={t("keywordSets.slot", { slot, shortcut })}
                  describedBy={isRefused ? errorId : undefined}
                  autoFocus={slot === focusSlot}
                />
              </div>
            );
          })}
        </div>
        {refusal !== null && (
          <p id={errorId} className={styles.error} role="alert">
            {setRefusalText(t, refusal)}
          </p>
        )}
        {failed && (
          <p className={styles.error} role="alert">
            {t("keywordSets.failed")}
          </p>
        )}
      </div>
    </Modal>
  );
}

/** Confirmation avant de supprimer un jeu (ses mots-clés restent). */
function DeleteSetDialog({
  set,
  onDelete,
  onClose,
}: {
  set: KeywordSetDto;
  onDelete: () => Promise<KeywordSetRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const [isDeleting, setIsDeleting] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  return (
    <Modal
      title={t("keywordSets.deleteTitle", { name: set.name })}
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          <Button
            variant="primary"
            disabled={isDeleting}
            onClick={() => {
              setIsDeleting(true);
              onDelete()
                .then((refusal) => {
                  // Sans refus, l'appelant ferme la fenêtre.
                  if (refusal === null) return;
                  setProblem(setRefusalText(t, refusal));
                  setIsDeleting(false);
                })
                .catch((error: unknown) => {
                  console.error("Failed to delete the keyword set", error);
                  setProblem(t("keywordSets.failed"));
                  setIsDeleting(false);
                });
            }}
          >
            {t("keywordSets.delete")}
          </Button>
        </>
      }
    >
      <p>{t("keywordSets.deleteExplanation")}</p>
      {problem !== null && (
        <p className={styles.error} role="alert">
          {problem}
        </p>
      )}
    </Modal>
  );
}

function setRefusalText(t: TFunction, refusal: KeywordSetRefusal): string {
  switch (refusal.kind) {
    case "notFound":
      return t("keywordSets.refusals.notFound");
    case "emptyName":
      return t("keywordSets.refusals.emptyName");
    case "tooLong":
      return t("keywordSets.refusals.tooLong");
    case "nameTaken":
      return t("keywordSets.refusals.nameTaken", { name: refusal.name });
    case "invalidSlots":
      return t("keywordSets.failed");
    case "slot":
      return slotRefusalText(t, refusal.slot, refusal.reason);
  }
}

function slotRefusalText(
  t: TFunction,
  slot: number,
  reason: KeywordSetSlotRefusal,
): string {
  switch (reason.kind) {
    case "several":
      return t("keywordSets.refusals.several", { slot });
    case "tooLong":
      return t("keywordSets.refusals.slotTooLong", { slot });
    case "forbiddenCharacter":
      return t("keywordSets.refusals.forbidden", { slot, name: reason.name });
    case "ambiguous":
      return t("keywordSets.refusals.ambiguous", { slot, name: reason.name });
  }
}
