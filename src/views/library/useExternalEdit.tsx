import { useState } from "react";
import type { ReactNode } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import { fetchExternalEditPreview } from "../../services/externalEditors";
import type {
  ExternalEditFailure,
  ExternalEditPreviewDto,
} from "../../services/externalEditors";
import type { EditActions } from "./editActions";

/** Au-delà, l'envoi est confirmé. */
const CONFIRM_ABOVE = 10;

/** Noms cités dans un message d'échec, au plus. */
const NAMES_SHOWN = 3;

/** Fenêtre affichée pendant « Modifier dans… ». */
type Step =
  | {
      kind: "ask";
      mediaIds: number[];
      editor: { id: string; name: string };
      preview: ExternalEditPreviewDto;
    }
  | { kind: "noEditor" }
  | { kind: "failed"; message: string };

/**
 * « Modifier dans… » : l'éditeur choisi (celui de
 * Cmd+E par défaut) reçoit les photos ; un JPEG, un TIFF, un PNG, un HEIC
 * ou un PSD part tel quel ou par une copie, au choix, et l'envoi de plus de
 * 10 photos est confirmé.
 *
 * @returns `start` pour lancer l'envoi, et la fenêtre à afficher.
 */
export function useExternalEdit(actions: EditActions): {
  start: (mediaIds: number[], editorId: string | null) => void;
  /** Signale un échec (ex. « Afficher dans le Finder »). */
  fail: (message: string) => void;
  dialog: ReactNode;
} {
  const { t } = useTranslation();
  const [step, setStep] = useState<Step | null>(null);
  const close = () => {
    setStep(null);
  };
  const fail = (message: string) => {
    setStep({ kind: "failed", message });
  };

  const failureMessage = (failure: ExternalEditFailure): string => {
    switch (failure.kind) {
      case "editorNotOpened":
        return t("externalEdit.editorNotOpened", { editor: failure.editor });
      case "nothingToSend":
        return t("externalEdit.nothingToSend");
      case "noEditor":
        return t("externalEdit.noEditor");
      case "catalogUnavailable":
        return t("externalEdit.catalogUnavailable");
    }
  };

  const send = (mediaIds: number[], editorId: string, copyNonRaw: boolean) => {
    const edit = actions.editInExternalEditor;
    if (edit === undefined) return;
    setStep(null);
    edit(mediaIds, editorId, copyNonRaw).then(
      (result) => {
        if (result.status === "error") {
          fail(failureMessage(result.error));
          return;
        }
        const failed = result.data.copyFailed;
        if (failed.length > 0) {
          fail(
            t("externalEdit.copyFailed", {
              ...counted(failed.length),
              names: failed.slice(0, NAMES_SHOWN).join(", "),
            }),
          );
        }
      },
      (error: unknown) => {
        console.error("Failed to reach the core", error);
        fail(t("externalEdit.catalogUnavailable"));
      },
    );
  };

  const start = (mediaIds: number[], editorId: string | null) => {
    const editors = actions.externalEditors;
    if (actions.editInExternalEditor === undefined || mediaIds.length === 0) {
      return;
    }
    if (
      editors === null ||
      editors === undefined ||
      editors.editors.length === 0
    ) {
      setStep({ kind: "noEditor" });
      return;
    }
    fetchExternalEditPreview(mediaIds).then(
      (preview) => {
        // Une version traitée retourne dans l'éditeur qui l'a produite
        //, sauf si un autre a été choisi.
        const id = editorId ?? preview.versionEditorId ?? editors.defaultId;
        const editor = editors.editors.find((candidate) => candidate.id === id);
        if (editor === undefined) {
          setStep({ kind: "noEditor" });
        } else if (preview.photos === 0) {
          fail(t("externalEdit.nothingToSend"));
        } else if (preview.copyable > 0 || preview.photos > CONFIRM_ABOVE) {
          setStep({ kind: "ask", mediaIds, editor, preview });
        } else {
          send(mediaIds, editor.id, false);
        }
      },
      (error: unknown) => {
        console.error("Failed to prepare the external edit", error);
        fail(t("externalEdit.catalogUnavailable"));
      },
    );
  };

  return { start, fail, dialog: renderStep() };

  function renderStep(): ReactNode {
    if (step === null) return null;
    switch (step.kind) {
      case "noEditor":
        return (
          <Modal
            title={t("externalEdit.noEditorTitle")}
            actions={
              <>
                <Button onClick={close}>{t("common.cancel")}</Button>
                {actions.openPreferences !== undefined && (
                  <Button
                    variant="primary"
                    autoFocus
                    onClick={() => {
                      close();
                      actions.openPreferences?.();
                    }}
                  >
                    {t("externalEdit.openPreferences")}
                  </Button>
                )}
              </>
            }
          >
            <p>{t("externalEdit.noEditor")}</p>
          </Modal>
        );
      case "failed":
        return (
          <Modal
            title={t("externalEdit.failedTitle")}
            actions={
              <Button variant="primary" autoFocus onClick={close}>
                {t("common.ok")}
              </Button>
            }
          >
            <p>{step.message}</p>
          </Modal>
        );
      case "ask":
        return <AskDialog step={step} onClose={close} onSend={send} />;
    }
  }
}

/**
 * Original ou copie (s'il y a autre chose que des RAW), et confirmation
 * au-delà de 10 photos.
 */
function AskDialog({
  step,
  onClose,
  onSend,
}: {
  step: Extract<Step, { kind: "ask" }>;
  onClose: () => void;
  onSend: (mediaIds: number[], editorId: string, copyNonRaw: boolean) => void;
}) {
  const { t } = useTranslation();
  const { mediaIds, editor, preview } = step;
  const raw = preview.photos - preview.copyable;
  const choose = preview.copyable > 0;
  const sendWith = (copyNonRaw: boolean) => () => {
    onSend(mediaIds, editor.id, copyNonRaw);
  };
  return (
    <Modal
      title={
        choose
          ? t("externalEdit.chooseTitle", { editor: editor.name })
          : t("externalEdit.confirmTitle", { editor: editor.name })
      }
      actions={
        <>
          <Button onClick={onClose}>{t("common.cancel")}</Button>
          {choose ? (
            <>
              <Button onClick={sendWith(false)}>
                {t("externalEdit.original")}
              </Button>
              <Button variant="primary" autoFocus onClick={sendWith(true)}>
                {t("externalEdit.copy")}
              </Button>
            </>
          ) : (
            <Button variant="primary" autoFocus onClick={sendWith(false)}>
              {t("externalEdit.open")}
            </Button>
          )}
        </>
      }
    >
      {preview.photos > CONFIRM_ABOVE && (
        <p>
          {t("externalEdit.confirm", {
            ...counted(preview.photos),
            editor: editor.name,
          })}
        </p>
      )}
      {choose && <p>{t("externalEdit.choose", counted(preview.copyable))}</p>}
      {choose && raw > 0 && (
        <p>{t("externalEdit.rawUnchanged", counted(raw))}</p>
      )}
      {preview.skipped > 0 && (
        <p>{t("externalEdit.skipped", counted(preview.skipped))}</p>
      )}
    </Modal>
  );
}
