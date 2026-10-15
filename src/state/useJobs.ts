import { useCallback, useEffect, useMemo, useState } from "react";

import { subscribeToJobs } from "../services/jobs";
import type { JobUpdate } from "../services/jobs";
import { isQuietXmpWrite } from "../services/xmp";

/** Tâches de fond connues de l'interface. */
export interface Jobs {
  /**
   * Tâches en cours, et tâches terminées dont le résultat n'a pas encore été
   * vu (une tâche annulée, ou de vignettes, disparaît dès sa fin). Dans
   * l'ordre de lancement.
   */
  jobs: JobUpdate[];
  /** Oublie une tâche terminée, une fois son résultat vu. */
  dismiss: (jobId: number) => void;
  /**
   * Nombre d'imports terminés (même annulés) depuis le démarrage : le
   * contenu du catalogue a pu changer quand il augmente.
   */
  finishedImports: number;
  /**
   * Nombre de mises à la corbeille terminées (même annulées ou en échec) :
   * des photos ont pu quitter le catalogue quand il augmente.
   */
  finishedTrash: number;
  /**
   * Nombre de mises à jour reçues des tâches de vignettes (progression et
   * fin) : de nouvelles vignettes sont prêtes quand il augmente.
   */
  thumbnailUpdates: number;
  /**
   * Nombre de remplissages des lieux terminés (même annulés ou en échec) :
   * le pays, la région ou la ville de photos ont pu changer quand il
   * augmente.
   */
  finishedPlaceLookups: number;
  /**
   * Nombre de recherches des fichiers manquants terminées : des photos ont
   * pu devenir manquantes, ou être retrouvées, quand il augmente.
   */
  finishedMissingChecks: number;
  /**
   * Nombre d'écritures des XMP terminées : les icônes d'état des vignettes
   * se relisent quand il change.
   */
  finishedXmpWrites: number;
  /**
   * Dernier ajout de photos traitées revenues d'un éditeur :
   * leur nombre, et une clé qui change à chaque ajout ; `null` : aucun.
   */
  editedVersionsAdded: { added: number; key: number } | null;
}

/** S'abonne une fois aux tâches de fond et tient leur liste à jour. */
export function useJobs(): Jobs {
  const [jobsById, setJobsById] = useState<ReadonlyMap<number, JobUpdate>>(
    () => new Map(),
  );
  const [finishedImports, setFinishedImports] = useState(0);
  const [thumbnailUpdates, setThumbnailUpdates] = useState(0);
  const [finishedTrash, setFinishedTrash] = useState(0);
  const [finishedPlaceLookups, setFinishedPlaceLookups] = useState(0);
  const [finishedMissingChecks, setFinishedMissingChecks] = useState(0);
  const [finishedXmpWrites, setFinishedXmpWrites] = useState(0);
  const [editedVersionsAdded, setEditedVersionsAdded] = useState<{
    added: number;
    key: number;
  } | null>(null);

  useEffect(() => {
    // Le cœur ne propose pas de désabonnement : après le démontage, les
    // messages reçus sont simplement ignorés.
    let isActive = true;
    subscribeToJobs((update) => {
      if (!isActive) return;
      if (update.kind.kind === "import" && update.status.status !== "running") {
        setFinishedImports((count) => count + 1);
      }
      if (update.kind.kind === "trash" && update.status.status !== "running") {
        setFinishedTrash((count) => count + 1);
      }
      if (update.kind.kind === "thumbnails") {
        setThumbnailUpdates((count) => count + 1);
      }
      if (update.kind.kind === "places" && update.status.status !== "running") {
        setFinishedPlaceLookups((count) => count + 1);
      }
      if (
        update.kind.kind === "missingFiles" &&
        update.status.status !== "running"
      ) {
        setFinishedMissingChecks((count) => count + 1);
      }
      if (
        update.kind.kind === "xmpWrite" &&
        update.status.status !== "running"
      ) {
        setFinishedXmpWrites((count) => count + 1);
      }
      if (
        update.status.status === "completed" &&
        update.status.result.kind === "editedVersions"
      ) {
        const { added } = update.status.result.summary;
        setEditedVersionsAdded((previous) => ({
          added,
          key: (previous?.key ?? 0) + 1,
        }));
      }
      setJobsById((previous) => {
        const next = new Map(previous);
        // Une tâche annulée, ou une tâche discrète (vignettes, lieux,
        // fichiers manquants) terminée, n'a pas de bilan à montrer.
        const isQuietEnd =
          ((update.kind.kind === "thumbnails" ||
            update.kind.kind === "places" ||
            update.kind.kind === "missingFiles" ||
            update.kind.kind === "editedVersions") &&
            update.status.status !== "running") ||
          // Une écriture des XMP automatique, ou sans rien à signaler, non
          // plus : les icônes des vignettes montrent les échecs.
          (update.kind.kind === "xmpWrite" &&
            update.kind.automatic &&
            update.status.status !== "running") ||
          (update.status.status === "completed" &&
            update.status.result.kind === "xmpWrite" &&
            isQuietXmpWrite(update.status.result.summary));
        if (update.status.status === "cancelled" || isQuietEnd) {
          next.delete(update.id);
        } else {
          next.set(update.id, update);
        }
        return next;
      });
    }).catch((error: unknown) => {
      console.error("Failed to subscribe to background jobs", error);
    });
    return () => {
      isActive = false;
    };
  }, []);

  const dismiss = useCallback((jobId: number) => {
    setJobsById((previous) => {
      const next = new Map(previous);
      next.delete(jobId);
      return next;
    });
  }, []);

  return useMemo(
    () => ({
      jobs: [...jobsById.values()],
      dismiss,
      finishedImports,
      finishedTrash,
      thumbnailUpdates,
      finishedPlaceLookups,
      finishedMissingChecks,
      finishedXmpWrites,
      editedVersionsAdded,
    }),
    [
      jobsById,
      dismiss,
      finishedImports,
      finishedTrash,
      thumbnailUpdates,
      finishedPlaceLookups,
      finishedMissingChecks,
      finishedXmpWrites,
      editedVersionsAdded,
    ],
  );
}
