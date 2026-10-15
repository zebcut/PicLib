import { useCallback, useEffect, useMemo, useRef, useState } from "react";

import {
  appliesDirectly,
  discardFileMetadata,
  readFileMetadata,
} from "../services/fileMetadata";
import type { FileMetadataStartFailure } from "../services/fileMetadata";
import type { JobUpdate } from "../services/jobs";

/** Échec au lancement d'une lecture, y compris un cœur qui ne répond pas. */
export type FileMetadataLaunchFailure =
  FileMetadataStartFailure | { kind: "coreUnreachable" };

/** « Lire les métadonnées des fichiers ». */
export interface FileMetadataReading {
  /** Lance la lecture des fichiers des médias indiqués. */
  start: (mediaIds: number[]) => void;
  /** Oublie, dans le cœur, une lecture terminée sans l'appliquer. */
  discard: (jobId: number) => void;
  /** Échec du dernier lancement, s'il y en a un à montrer. */
  startFailure: FileMetadataLaunchFailure | null;
  /** Ferme le message d'échec. */
  dismissStartFailure: () => void;
}

/**
 * Lance la lecture des métadonnées des fichiers ; sa progression et son
 * bilan arrivent par l'abonnement aux tâches de fond (`useJobs`). Une
 * lecture d'une seule photo qui trouve quelque chose à compléter est
 * appliquée aussitôt, sans confirmation (annulable avec Cmd+Z) ; au-delà,
 * le bilan est montré d'abord (`FileMetadataResult`).
 *
 * @param apply complète les médias d'après une lecture (annulable).
 * @param dismiss oublie une tâche terminée, une fois traitée.
 */
export function useFileMetadataReading(
  jobs: JobUpdate[],
  apply: (jobId: number) => void,
  dismiss: (jobId: number) => void,
): FileMetadataReading {
  const [startFailure, setStartFailure] =
    useState<FileMetadataLaunchFailure | null>(null);
  // Lectures déjà appliquées : la liste des tâches peut être relue avant
  // que la tâche ne soit oubliée.
  const applied = useRef(new Set<number>());

  useEffect(() => {
    for (const job of jobs) {
      const { status } = job;
      if (
        status.status !== "completed" ||
        status.result.kind !== "fileMetadata" ||
        !appliesDirectly(status.result.summary) ||
        applied.current.has(job.id)
      ) {
        continue;
      }
      applied.current.add(job.id);
      apply(job.id);
      dismiss(job.id);
    }
  }, [jobs, apply, dismiss]);

  const start = useCallback((mediaIds: number[]) => {
    if (mediaIds.length === 0) return;
    readFileMetadata(mediaIds).then(
      (result) => {
        if (result.status === "error") setStartFailure(result.error);
      },
      (error: unknown) => {
        console.error("Failed to start reading the files' metadata", error);
        setStartFailure({ kind: "coreUnreachable" });
      },
    );
  }, []);

  const discard = useCallback((jobId: number) => {
    discardFileMetadata(jobId).catch((error: unknown) => {
      console.error("Failed to discard the files' metadata", error);
    });
  }, []);

  const dismissStartFailure = useCallback(() => {
    setStartFailure(null);
  }, []);

  return useMemo(
    () => ({ start, discard, startFailure, dismissStartFailure }),
    [start, discard, startFailure, dismissStartFailure],
  );
}
