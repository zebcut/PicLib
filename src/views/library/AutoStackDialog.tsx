import { useCallback, useId, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";
import { fetchAutoStacking } from "../../services/stacks";
import type {
  AutoStackMethod,
  AutoStackScope,
  StackOperation,
} from "../../services/stacks";
import { useBackendData } from "../../state/useBackendData";
import styles from "./FolderDeleteDialog.module.css";

/** Écarts proposés entre deux photos d'une rafale, en secondes. */
const BURST_GAPS = [1, 2, 3, 5, 10] as const;

/** Écart par défaut entre deux photos d'une rafale. */
const DEFAULT_BURST_GAP = 2;

/** Façon d'empiler automatiquement : les paires RAW + JPEG, ou les rafales. */
export type AutoStackKind = AutoStackMethod["kind"];

/** Empilement automatique demandé : sur quoi, et comment. */
export interface AutoStackRequest {
  scope: AutoStackScope;
  /** Nom du dossier, pour une demande sur un dossier. */
  folderName?: string;
  kind: AutoStackKind;
}

/**
 * Résumé d'un empilement automatique à confirmer : le
 * nombre de piles et de photos, sur la sélection ou un dossier et ses
 * sous-dossiers ; pour les rafales, l'écart entre deux photos se règle
 * ici. Empiler s'annule par Cmd+Z.
 */
export function AutoStackDialog({
  request,
  onStack,
  onClose,
}: {
  request: AutoStackRequest;
  onStack: (operation: StackOperation) => void;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const gapId = useId();
  const { scope, kind } = request;
  const [gapSeconds, setGapSeconds] = useState(DEFAULT_BURST_GAP);
  const method: AutoStackMethod =
    kind === "bursts" ? { kind, gapSeconds } : { kind };
  const load = useCallback(
    () =>
      fetchAutoStacking(
        scope,
        kind === "bursts" ? { kind, gapSeconds } : { kind },
      ),
    [scope, kind, gapSeconds],
  );
  const preview = useBackendData(load, "auto stacking");
  const where =
    request.folderName === undefined
      ? t("stacks.auto.inSelection")
      : t("stacks.auto.inFolder", { name: request.folderName });
  const isEmpty = preview.status === "ready" && preview.data.stacks === 0;

  return (
    <Modal
      title={t(`stacks.auto.${kind}Title`)}
      actions={
        (isEmpty && kind === "pairs") || preview.status === "error" ? (
          <Button variant="primary" autoFocus onClick={onClose}>
            {t("common.ok")}
          </Button>
        ) : (
          <>
            <Button onClick={onClose}>{t("common.cancel")}</Button>
            <Button
              variant="primary"
              autoFocus
              disabled={preview.status !== "ready" || isEmpty}
              onClick={() => {
                onStack({ kind: "auto", scope, method });
                onClose();
              }}
            >
              {t("stacks.auto.stack")}
            </Button>
          </>
        )
      }
    >
      {kind === "bursts" && (
        <p>
          <label htmlFor={gapId}>{t("stacks.auto.burstGap")}</label>{" "}
          <select
            id={gapId}
            value={gapSeconds}
            onChange={(event) => {
              setGapSeconds(Number(event.currentTarget.value));
            }}
          >
            {BURST_GAPS.map((choice) => (
              <option key={choice} value={choice}>
                {t("stacks.auto.seconds", counted(choice))}
              </option>
            ))}
          </select>{" "}
          {t("folderAnalysis.burstGapAfter")}
        </p>
      )}
      {preview.status === "error" && (
        <p role="alert">{t("stacks.auto.loadFailed")}</p>
      )}
      {preview.status === "ready" && (
        <>
          <p>
            {isEmpty
              ? t(`stacks.auto.${kind}None`, { where })
              : t(`stacks.auto.${kind}Found`, {
                  where,
                  photos: t("stacks.auto.photos", counted(preview.data.photos)),
                  ...counted(preview.data.stacks),
                })}
          </p>
          {!isEmpty && (
            <p className={styles.explanation}>
              {t(`stacks.auto.${kind}Explanation`)}
            </p>
          )}
        </>
      )}
    </Modal>
  );
}
