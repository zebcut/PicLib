import type { TFunction } from "i18next";

import { jobFailureMessage } from "../../components/jobs/Lines";
import type { ImportLaunchFailure } from "../../state/useImport";
import type { ImportAnalysis } from "./useImportAnalysis";

/** Message expliquant pourquoi l'import n'a pas démarré. */
export function importStartMessage(
  t: TFunction,
  failure: ImportLaunchFailure,
): string {
  switch (failure.kind) {
    case "coreUnreachable":
      return t("folderAnalysis.coreUnreachable");
    case "unknownAnalysis":
      return t("importResult.unknownAnalysis");
    case "nothingSelected":
      return t("importScreen.nothingSelected");
    case "catalogUnavailable":
      return t("importResult.catalogUnavailable");
    case "cameraCard":
      return t("folderAnalysis.cameraCard");
    case "templateNotFound":
      return t("importResult.templateNotFound");
    case "destinationOverlap":
      return t("importResult.destinationOverlap", {
        path: failure.destination,
      });
    case "destinationOnCameraCard":
      return t("importResult.destinationOnCameraCard", {
        path: failure.destination,
      });
    case "backupUnavailable":
      return t("importScreen.backup.unavailable");
    case "jobNotStarted":
      return t("importResult.jobNotStarted");
  }
}

/** Message expliquant pourquoi la source n'a pas pu être analysée. */
export function analysisFailureMessage(
  t: TFunction,
  analysis: Extract<ImportAnalysis, { status: "failed" }>,
): string {
  const { failure } = analysis;
  if (failure === null) return t("folderAnalysis.crashed");
  if (failure.kind === "startFailed") return t("folderAnalysis.jobNotStarted");
  return jobFailureMessage(t, failure);
}
