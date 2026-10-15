import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import type {
  KeywordImportPreviewDto,
  KeywordLineIssueReasonDto,
  KeywordTransferFailure,
} from "../../services/keywordTransfer";
import type { KeywordTransferDialogState } from "../../state/useKeywordTransfer";
import styles from "./KeywordTransferDialog.module.css";

/**
 * Fenêtre de l'import ou de l'export de la liste des mots-clés :
 * aperçu à confirmer avant d'importer (règle des opérations de masse),
 * confirmation d'un export, ou raison d'un échec.
 */
export function KeywordTransferDialog({
  dialog,
  onImport,
  onClose,
}: {
  dialog: KeywordTransferDialogState;
  onImport: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  switch (dialog.kind) {
    case "preview":
      return (
        <ImportPreview
          preview={dialog.preview}
          onImport={onImport}
          onClose={onClose}
        />
      );
    case "exported":
      return (
        <Modal
          title={t("keywordTransfer.exportedTitle")}
          actions={
            <Button variant="primary" autoFocus onClick={onClose}>
              {t("common.ok")}
            </Button>
          }
        >
          <p>
            {t("keywordTransfer.exported", { file: dialog.result.fileName })}
          </p>
          {dialog.result.replaced && <p>{t("keywordTransfer.replaced")}</p>}
        </Modal>
      );
    case "failed":
      return (
        <Modal
          title={t("keywordTransfer.failedTitle")}
          actions={
            <Button variant="primary" autoFocus onClick={onClose}>
              {t("common.ok")}
            </Button>
          }
        >
          <p role="alert">{failureText(t, dialog.failure)}</p>
        </Modal>
      );
  }
}

/** Ce que l'import fera, avant de le faire. */
function ImportPreview({
  preview,
  onImport,
  onClose,
}: {
  preview: KeywordImportPreviewDto;
  onImport: () => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const hidden = preview.issueCount - preview.issues.length;
  const canImport = preview.created > 0;
  return (
    <Modal
      title={t("keywordTransfer.previewTitle", { file: preview.fileName })}
      actions={
        canImport ? (
          <>
            <Button onClick={onClose}>{t("common.cancel")}</Button>
            <Button variant="primary" autoFocus onClick={onImport}>
              {t("keywordTransfer.import")}
            </Button>
          </>
        ) : (
          <Button variant="primary" autoFocus onClick={onClose}>
            {t("common.ok")}
          </Button>
        )
      }
    >
      <ul className={styles.counts}>
        <li>{t("keywordTransfer.created", counted(preview.created))}</li>
        <li>{t("keywordTransfer.existing", counted(preview.existing))}</li>
        {preview.synonyms > 0 && (
          <li>{t("keywordTransfer.synonyms", counted(preview.synonyms))}</li>
        )}
        {preview.issueCount > 0 && (
          <li>{t("keywordTransfer.issues", counted(preview.issueCount))}</li>
        )}
      </ul>
      {preview.issues.length > 0 && (
        <ul
          className={styles.issues}
          aria-label={t("keywordTransfer.issuesLabel")}
        >
          {preview.issues.map((issue) => (
            <li key={issue.line}>
              {t("keywordTransfer.issueLine", {
                line: issue.line,
                reason: issueText(t, issue.reason),
              })}
            </li>
          ))}
          {hidden > 0 && (
            <li>{t("keywordTransfer.moreIssues", counted(hidden))}</li>
          )}
        </ul>
      )}
      <p className={styles.hint}>
        {canImport
          ? t("keywordTransfer.importHint")
          : t("keywordTransfer.nothingNew")}
      </p>
    </Modal>
  );
}

function issueText(t: TFunction, reason: KeywordLineIssueReasonDto): string {
  switch (reason.kind) {
    case "tooDeep":
      return t("keywordTransfer.reasons.tooDeep");
    case "parentIgnored":
      return t("keywordTransfer.reasons.parentIgnored");
    case "emptyName":
      return t("keywordTransfer.reasons.emptyName");
    case "tooLong":
      return t("keywordTransfer.reasons.tooLong");
    case "forbiddenCharacter":
      return t("keywordTransfer.reasons.forbidden", { name: reason.name });
  }
}

function failureText(
  t: TFunction,
  failure: KeywordTransferFailure | null,
): string {
  switch (failure?.kind) {
    case "tooLarge":
      return t("keywordTransfer.failures.tooLarge", {
        megabytes: failure.megabytes,
      });
    case "notUtf8":
      return t("keywordTransfer.failures.notUtf8");
    case "cannotReplace":
      return t("keywordTransfer.failures.cannotReplace");
    case "fileUnavailable":
    case "unsupportedLocation":
      return t("keywordTransfer.failures.file");
    case "catalogUnavailable":
    case "nothingToImport":
    case undefined:
      return t("keywordTransfer.failures.other");
  }
}
