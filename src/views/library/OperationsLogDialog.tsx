import { useCallback, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { formatCount, formatInstant } from "../../i18n/format";
import {
  fetchOperationItems,
  fetchOperations,
} from "../../services/operationsLog";
import type { OperationDto } from "../../services/operationsLog";
import { useBackendData } from "../../state/useBackendData";
import styles from "./OperationsLogDialog.module.css";

/**
 * Journal des opérations sur les fichiers : date, opération,
 * fichiers, résultat ; un clic sur une opération montre ses fichiers.
 */
export function OperationsLogDialog({ onClose }: { onClose: () => void }) {
  const { t } = useTranslation();
  const operations = useBackendData(fetchOperations, "operations journal");
  const [openId, setOpenId] = useState<number | null>(null);

  return (
    <Modal
      title={t("operationsLog.title")}
      actions={
        <Button variant="primary" autoFocus onClick={onClose}>
          {t("common.close")}
        </Button>
      }
    >
      {operations.status === "loading" && <p>{t("operationsLog.loading")}</p>}
      {operations.status === "error" && (
        <p role="alert">{t("operationsLog.loadFailed")}</p>
      )}
      {operations.status === "ready" && operations.data.length === 0 && (
        <p>{t("operationsLog.empty")}</p>
      )}
      {operations.status === "ready" && operations.data.length > 0 && (
        <div className={styles.scroll}>
          <table className={styles.table}>
            <thead>
              <tr>
                <th>{t("operationsLog.date")}</th>
                <th>{t("operationsLog.operation")}</th>
                <th>{t("operationsLog.files")}</th>
                <th>{t("operationsLog.result")}</th>
              </tr>
            </thead>
            <tbody>
              {operations.data.map((operation) => (
                <OperationRow
                  key={operation.id}
                  operation={operation}
                  isOpen={openId === operation.id}
                  onToggle={() => {
                    setOpenId(openId === operation.id ? null : operation.id);
                  }}
                />
              ))}
            </tbody>
          </table>
        </div>
      )}
    </Modal>
  );
}

function OperationRow({
  operation,
  isOpen,
  onToggle,
}: {
  operation: OperationDto;
  isOpen: boolean;
  onToggle: () => void;
}) {
  const { t } = useTranslation();
  const problems = operation.missing + operation.failed + operation.pending;
  const total = operation.done + problems;
  return (
    <>
      <tr>
        <td>{formatInstant(operation.startedAt)}</td>
        <td>
          <button
            type="button"
            className={styles.rowButton}
            aria-expanded={isOpen}
            onClick={onToggle}
          >
            {isOpen ? "▾ " : "▸ "}
            {t(`operationsLog.kind.${operation.kind}`)}
          </button>
        </td>
        <td>
          {t("operationsLog.counts", {
            done: formatCount(operation.done),
            total: formatCount(total),
          })}
        </td>
        <td className={problems > 0 ? styles.problem : undefined}>
          {t(`operationsLog.status.${operation.status}`)}
        </td>
      </tr>
      {isOpen && (
        <tr>
          <td colSpan={4}>
            <OperationItems id={operation.id} />
          </td>
        </tr>
      )}
    </>
  );
}

function OperationItems({ id }: { id: number }) {
  const { t } = useTranslation();
  const load = useCallback(() => fetchOperationItems(id), [id]);
  const items = useBackendData(load, "operation items");
  if (items.status === "loading") return <p>{t("operationsLog.loading")}</p>;
  if (items.status === "error") {
    return <p role="alert">{t("operationsLog.loadFailed")}</p>;
  }
  return (
    <ul className={styles.items}>
      {items.data.map((item, index) => (
        <li
          key={`${String(index)}-${item.source}`}
          className={item.result === "done" ? undefined : styles.problem}
        >
          {item.destination === null
            ? item.source
            : `${item.source} → ${item.destination}`}{" "}
          ({t(`operationsLog.itemResult.${item.result}`)}
          {item.detail === null ? "" : ` : ${item.detail}`})
        </li>
      ))}
    </ul>
  );
}
