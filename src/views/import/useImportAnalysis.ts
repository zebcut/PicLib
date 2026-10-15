import { useEffect, useState } from "react";

import {
  analyzeImportSource,
  fetchAnalyzedFiles,
  forgetAnalysis,
} from "../../services/importSources";
import type { ImportFileDto } from "../../services/importSources";
import { cancelJob } from "../../services/jobs";
import type {
  FolderAnalysisSummary,
  JobFailure,
  JobUpdate,
} from "../../services/jobs";

/** Analyse de la source choisie dans l'écran d'import. */
export type ImportAnalysis =
  | { status: "none" }
  | { status: "starting" }
  | {
      status: "running";
      jobId: number;
      progress: number;
      total: number | null;
    }
  | {
      status: "ready";
      jobId: number;
      files: ImportFileDto[];
      summary: FolderAnalysisSummary;
    }
  | {
      status: "failed";
      /** `null` : erreur interne ou cœur injoignable. */
      failure: JobFailure | { kind: "startFailed" } | null;
    };

/**
 * Analyse la source `sourceId` (aucune si `null`), avec ou sans ses
 * sous-dossiers : lance la tâche, suit sa
 * progression, puis lit ses fichiers. Changer de source, ou fermer l'écran,
 * arrête l'analyse précédente et l'oublie (ses vignettes sont effacées).
 */
export function useImportAnalysis(
  sourceId: number | null,
  includeSubfolders: boolean,
  jobs: readonly JobUpdate[],
  onDismissJob: (jobId: number) => void,
): ImportAnalysis {
  // Analyse lancée pour une source ; ignorée dès qu'une autre est choisie.
  const [launch, setLaunch] = useState<{
    sourceId: number;
    includeSubfolders: boolean;
    jobId: number | null;
  } | null>(null);
  const [files, setFiles] = useState<{
    jobId: number;
    files: ImportFileDto[] | null;
  } | null>(null);

  useEffect(() => {
    if (sourceId === null) return;
    let isActive = true;
    let started: number | null = null;
    analyzeImportSource(sourceId, includeSubfolders).then(
      (result) => {
        if (result.status === "error") {
          if (isActive) setLaunch({ sourceId, includeSubfolders, jobId: null });
          return;
        }
        started = result.data;
        if (isActive)
          setLaunch({ sourceId, includeSubfolders, jobId: result.data });
        else release(result.data);
      },
      (error: unknown) => {
        console.error("Failed to start the source analysis", error);
        if (isActive) setLaunch({ sourceId, includeSubfolders, jobId: null });
      },
    );
    return () => {
      isActive = false;
      if (started !== null) release(started);
    };
  }, [sourceId, includeSubfolders]);

  const current =
    launch?.sourceId === sourceId &&
    launch.includeSubfolders === includeSubfolders
      ? launch
      : null;
  const jobId = current?.jobId ?? null;
  const startFailed = current !== null && current.jobId === null;
  const job = jobs.find((candidate) => candidate.id === jobId);

  // Le résultat de l'analyse est montré ici : il est gardé, et la tâche
  // oubliée dès qu'elle est finie.
  const [kept, setKept] = useState<{
    jobId: number;
    summary: FolderAnalysisSummary | null;
    failure: JobFailure | null;
  } | null>(null);
  if (
    job !== undefined &&
    job.status.status !== "running" &&
    kept?.jobId !== job.id
  ) {
    setKept({
      jobId: job.id,
      summary:
        job.status.status === "completed" &&
        job.status.result.kind === "folderAnalysis"
          ? job.status.result.summary
          : null,
      failure: job.status.status === "failed" ? job.status.failure : null,
    });
  }
  useEffect(() => {
    if (kept !== null) onDismissJob(kept.jobId);
  }, [kept, onDismissJob]);

  // Analyse réussie : ses fichiers sont lus une fois. L'effet suit le
  // résultat gardé, pas la tâche : celle-ci est oubliée aussitôt finie, et
  // sa disparition ne doit pas abandonner la lecture en cours.
  const completedJobId =
    kept !== null && kept.jobId === jobId && kept.summary !== null
      ? jobId
      : null;
  useEffect(() => {
    if (completedJobId === null) return;
    let isActive = true;
    fetchAnalyzedFiles(completedJobId).then(
      (list) => {
        if (isActive) setFiles({ jobId: completedJobId, files: list });
      },
      (error: unknown) => {
        console.error("Failed to read the analyzed files", error);
        if (isActive) setFiles({ jobId: completedJobId, files: null });
      },
    );
    return () => {
      isActive = false;
    };
  }, [completedJobId]);

  if (sourceId === null) return { status: "none" };
  if (startFailed)
    return { status: "failed", failure: { kind: "startFailed" } };
  if (jobId === null) return { status: "starting" };
  if (job?.status.status === "running") {
    return {
      status: "running",
      jobId,
      progress: job.status.progress,
      total: job.status.total,
    };
  }
  if (kept?.jobId !== jobId) return { status: "starting" };
  if (kept.summary === null) return { status: "failed", failure: kept.failure };
  if (files?.jobId !== jobId) {
    return { status: "running", jobId, progress: 0, total: null };
  }
  if (files.files === null) return { status: "failed", failure: null };
  return { status: "ready", jobId, files: files.files, summary: kept.summary };
}

/** Arrête une analyse (si elle tourne encore) et l'oublie. */
function release(jobId: number): void {
  cancelJob(jobId)
    .then(() => forgetAnalysis(jobId))
    .catch((error: unknown) => {
      console.error("Failed to release the source analysis", error);
    });
}
