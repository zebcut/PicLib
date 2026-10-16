import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { formatCount, formatPercent, progressCounts } from "../../i18n/format";
import { cancelJob } from "../../services/jobs";
import type { JobKind, JobUpdate } from "../../services/jobs";
import { lastPathSegment } from "../../utils/path";
import { Button } from "../Button";
import styles from "./JobIndicator.module.css";

/**
 * Indicateur discret des tâches de fond en cours, avec leur progression et un
 * bouton pour les annuler. N'affiche rien quand aucune tâche ne tourne.
 *
 * A job that knows its total also shows a progress bar, its percentage and,
 * once it can be estimated, the time left.
 *
 * @param remaining estimated milliseconds left, by job id.
 */
export function JobIndicator({
  jobs,
  remaining = new Map(),
}: {
  jobs: JobUpdate[];
  remaining?: ReadonlyMap<number, number>;
}) {
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
              {job.total !== null &&
                job.total > 0 &&
                ` · ${formatPercent(job.progress / job.total)}`}
            </span>
            {job.total !== null && job.total > 0 && (
              <ProgressBar progress={job.progress} total={job.total} />
            )}
            {remaining.has(job.id) && (
              <span className={styles.progress}>
                {remainingText(t, remaining.get(job.id) ?? 0)}
              </span>
            )}
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
    case "catalogCheck":
      return t("jobs.catalogCheck");
    case "catalogCleanup":
      return t("jobs.catalogCleanup");
  }
}

function progressText(
  t: TFunction,
  kind: JobKind,
  progress: number,
  total: number | null,
): string {
  const isMaintenance =
    kind.kind === "catalogCheck" || kind.kind === "catalogCleanup";
  // Integrity check, backup, rebuild: no count to show.
  if (isMaintenance && total === null) return t("jobs.working");
  if (total === null) {
    return t("jobs.filesFound", {
      count: progress,
      formatted: formatCount(progress),
    });
  }
  const counts = progressCounts(progress, total);
  switch (kind.kind) {
    case "folderAnalysis":
      return t("importScreen.readingDates", counts);
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
    case "catalogCheck":
    case "catalogCleanup":
      return t("jobs.cacheImagesProcessed", counts);
  }
}

function ProgressBar({ progress, total }: { progress: number; total: number }) {
  const ratio = Math.min(1, Math.max(0, progress / total));
  return (
    <div
      className={styles.bar}
      role="progressbar"
      aria-valuemin={0}
      aria-valuemax={total}
      aria-valuenow={Math.min(progress, total)}
    >
      <div
        className={styles.fill}
        style={{ width: `${String(ratio * 100)}%` }}
      />
    </div>
  );
}

/** "About 12 min left", rounded up to the minute. */
function remainingText(t: TFunction, milliseconds: number): string {
  const minutes = Math.ceil(milliseconds / 60_000);
  if (milliseconds < 60_000) return t("jobs.remainingUnderMinute");
  if (minutes < 60) return t("jobs.remainingMinutes", { minutes });
  return t("jobs.remainingHours", {
    hours: Math.floor(minutes / 60),
    minutes: String(minutes % 60).padStart(2, "0"),
  });
}
