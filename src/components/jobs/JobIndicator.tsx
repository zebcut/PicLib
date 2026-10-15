import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { formatCount } from "../../i18n/format";
import { cancelJob } from "../../services/jobs";
import type { JobKind, JobUpdate } from "../../services/jobs";
import { lastPathSegment } from "../../utils/path";
import { Button } from "../Button";
import styles from "./JobIndicator.module.css";

/**
 * Indicateur discret des tâches de fond en cours, avec leur progression et un
 * bouton pour les annuler. N'affiche rien quand aucune tâche ne tourne.
 */
export function JobIndicator({ jobs }: { jobs: JobUpdate[] }) {
  const { t } = useTranslation();

  const running = jobs.flatMap((job) =>
    job.status.status === "running" && !isDiscreet(job.kind)
      ? [{ id: job.id, kind: job.kind, ...job.status }]
      : [],
  );
  if (running.length === 0) return null;

  return (
    <aside
      className={styles.indicator}
      role="status"
      aria-label={t("jobs.indicatorLabel")}
    >
      {running.map((job) => (
        <div key={job.id} className={styles.job}>
          <span className={styles.activity} aria-hidden="true" />
          <div className={styles.texts}>
            <span className={styles.title}>{jobTitle(t, job.kind)}</span>
            <span className={styles.progress}>
              {progressText(t, job.kind, job.progress, job.total)}
            </span>
          </div>
          <Button
            title={t("jobs.cancelTooltip")}
            onClick={() => {
              cancelJob(job.id).catch((error: unknown) => {
                console.error("Failed to cancel the job", error);
              });
            }}
          >
            {t("common.cancel")}
          </Button>
        </div>
      ))}
    </aside>
  );
}

/**
 * Écriture automatique des XMP de quelques photos, après une modification
 * : trop brève pour être montrée.
 */
const DISCREET_AUTOMATIC_WRITES = 50;

function isDiscreet(kind: JobKind): boolean {
  return (
    (kind.kind === "xmpWrite" &&
      kind.automatic &&
      kind.count < DISCREET_AUTOMATIC_WRITES) ||
    // Photos traitées revenues d'un éditeur : un message
    // les annonce une fois ajoutées.
    kind.kind === "editedVersions"
  );
}

function jobTitle(t: TFunction, kind: JobKind): string {
  switch (kind.kind) {
    case "folderAnalysis":
      return t("jobs.folderAnalysis", { folder: lastPathSegment(kind.folder) });
    case "import":
      return t(kind.copy ? "jobs.importCopy" : "jobs.import", {
        folder: lastPathSegment(kind.folder),
      });
    case "thumbnails":
      return t("jobs.thumbnails");
    case "places":
      return t("jobs.places");
    case "trash":
      return t("jobs.trash");
    case "fileMetadata":
      return t("jobs.fileMetadata");
    case "missingFiles":
      return t("jobs.missingFiles");
    case "xmpWrite":
      return t("jobs.xmpWrite");
    case "editedVersions":
      return t("jobs.editedVersions");
  }
}

function progressText(
  t: TFunction,
  kind: JobKind,
  progress: number,
  total: number | null,
): string {
  const done = formatCount(progress);
  if (kind.kind === "folderAnalysis" || total === null) {
    return t("jobs.filesFound", { count: progress, formatted: done });
  }
  const counts = { count: progress, done, total: formatCount(total) };
  switch (kind.kind) {
    case "import":
      return t("jobs.filesProcessed", counts);
    case "trash":
      return t("jobs.trashProcessed", counts);
    case "thumbnails":
      return t("jobs.thumbnailsProcessed", counts);
    case "places":
      return t("jobs.placesProcessed", counts);
    case "fileMetadata":
      return t("jobs.fileMetadataProcessed", counts);
    case "missingFiles":
      return t("jobs.missingFilesProcessed", counts);
    case "xmpWrite":
      return t("jobs.xmpWriteProcessed", counts);
    case "editedVersions":
      return t("jobs.filesProcessed", counts);
  }
}
