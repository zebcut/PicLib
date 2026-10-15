import { useCallback } from "react";
import { useTranslation } from "react-i18next";

import styles from "../../components/jobs/JobDialogs.module.css";
import { counted } from "../../i18n/format";
import { fetchDestinationPreview } from "../../services/importSources";
import type { DestinationPreviewRequest } from "../../services/importSources";
import { useBackendData } from "../../state/useBackendData";
import { useThrottledValue } from "../../state/useThrottledValue";
import screenStyles from "./ImportView.module.css";

/** Intervalle minimal entre deux aperçus, pendant la saisie. */
const PREVIEW_THROTTLE_MS = 300;

/**
 * Aperçu de l'arborescence qui sera créée : chaque dossier avec
 * le nombre de fichiers qui y seront copiés, les nouveaux mis en évidence ;
 * et les premiers noms, pour un renommage.
 */
export function ImportTreePreview({
  analysisJobId,
  request,
}: {
  analysisJobId: number;
  request: DestinationPreviewRequest;
}) {
  const { t } = useTranslation();
  // Le cœur n'est pas interrogé à chaque touche ni à chaque case cochée.
  const shown = useThrottledValue(
    JSON.stringify({ analysisJobId, request }),
    PREVIEW_THROTTLE_MS,
  );
  const load = useCallback(() => {
    const asked = JSON.parse(shown) as {
      analysisJobId: number;
      request: DestinationPreviewRequest;
    };
    return fetchDestinationPreview(asked.analysisJobId, asked.request);
  }, [shown]);
  const preview = useBackendData(load, "destination preview");

  if (request.selected.length === 0) return null;
  return (
    <section
      className={screenStyles.group}
      aria-label={t("importScreen.tree.title")}
    >
      <h3 className={screenStyles.subheading}>
        {t("importScreen.tree.title")}
      </h3>
      {preview.status === "error" && (
        <p className={styles.warning}>{t("importScreen.tree.unavailable")}</p>
      )}
      {preview.status === "ready" && (
        <>
          <ul className={screenStyles.tree}>
            {preview.data.folders.map((folder) => (
              <li
                key={folder.path}
                className={folder.isNew ? screenStyles.newFolder : undefined}
                style={{ paddingLeft: `${String(folder.depth)}em` }}
              >
                {folder.path === "" ? preview.data.destination : folder.name}
                {folder.fileCount > 0 &&
                  ` — ${t("importScreen.tree.files", counted(folder.fileCount))}`}
                {folder.isNew && (
                  <span className={screenStyles.newTag}>
                    {t("importScreen.tree.new")}
                  </span>
                )}
              </li>
            ))}
          </ul>
          {preview.data.renamed.length > 0 && (
            <ul
              className={screenStyles.tree}
              aria-label={t("importScreen.tree.renamed")}
            >
              {preview.data.renamed.map((example) => (
                <li key={example.from}>
                  {example.from} → {example.to}
                </li>
              ))}
            </ul>
          )}
          {preview.data.fileDateCount > 0 && (
            <p className={styles.secondary}>
              {t(
                "importScreen.tree.fileDates",
                counted(preview.data.fileDateCount),
              )}
            </p>
          )}
        </>
      )}
    </section>
  );
}
