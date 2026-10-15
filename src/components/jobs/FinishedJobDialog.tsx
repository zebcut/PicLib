import type { JobUpdate } from "../../services/jobs";
import { lastPathSegment } from "../../utils/path";
import { FileMetadataResult } from "./FileMetadataResult";
import { ImportResult } from "./ImportResult";
import { TrashResult } from "./TrashResult";
import { XmpWriteResult } from "./XmpWriteResult";

/**
 * Résultat de la plus ancienne tâche terminée dont le résultat n'a pas
 * encore été vu. Les résultats s'affichent ainsi un par un ; celui d'une
 * analyse de source est montré par l'écran d'import.
 */
export function FinishedJobDialog({
  jobs,
  onDismiss,
  onApplyFileMetadata,
  onDiscardFileMetadata,
}: {
  jobs: JobUpdate[];
  onDismiss: (jobId: number) => void;
  /** Complète les photos d'après une lecture de leurs fichiers (annulable). */
  onApplyFileMetadata?: (jobId: number) => void;
  /** Oublie une lecture de fichiers sans l'appliquer. */
  onDiscardFileMetadata?: (jobId: number) => void;
}) {
  const finished = jobs.find(
    (job) =>
      job.kind.kind !== "folderAnalysis" &&
      job.status.status !== "running" &&
      job.status.status !== "cancelled",
  );
  if (finished === undefined) return null;
  const { status } = finished;
  if (status.status === "running" || status.status === "cancelled") {
    return null;
  }

  const close = () => {
    onDismiss(finished.id);
  };

  switch (finished.kind.kind) {
    case "folderAnalysis":
      return null;
    case "import":
      return (
        <ImportResult
          jobId={finished.id}
          folderName={lastPathSegment(finished.kind.folder)}
          card={finished.kind.card}
          status={status}
          onClose={close}
        />
      );
    case "thumbnails":
    case "places":
    case "missingFiles":
    case "editedVersions":
      // Tâches discrètes : pas de bilan à montrer (voir useJobs).
      return null;
    case "trash":
      return <TrashResult status={status} onClose={close} />;
    case "xmpWrite":
      return <XmpWriteResult status={status} onClose={close} />;
    case "fileMetadata":
      return (
        <FileMetadataResult
          status={status}
          onApply={() => {
            onApplyFileMetadata?.(finished.id);
            close();
          }}
          onClose={() => {
            onDiscardFileMetadata?.(finished.id);
            close();
          }}
        />
      );
  }
}
