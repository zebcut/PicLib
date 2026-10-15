import type { TFunction } from "i18next";

import type { EditChangeDto, EditSummaryDto } from "../services/edits";
import { counted } from "./format";

/**
 * Description d'une modification, pour les messages et les menus :
 * « Note : 3 étoiles », « Marqueur : rejetée », « Rotation : à gauche ».
 */
export function describeChange(t: TFunction, change: EditChangeDto): string {
  switch (change.kind) {
    case "flag":
      return t("triage.flagChange", {
        value:
          change.flag === null
            ? t("triage.unflagged")
            : t(`triage.${change.flag}`),
      });
    case "rating":
      return t("triage.ratingChange", {
        value:
          change.stars === 0
            ? t("triage.noRating")
            : t("triage.stars", { count: change.stars }),
      });
    case "colorLabel":
      return t("triage.colorChange", {
        value:
          change.color === null
            ? t("triage.noColor")
            : t(`triage.${change.color}`),
      });
    case "rotation":
      return t("triage.rotationChange", {
        value: t(`triage.${change.direction}`),
      });
    case "removal":
      return t("triage.removal");
    case "metadata":
      return t("metadata.changed", {
        field: t(`metadata.fields.${change.field}`),
      });
    case "captureTime":
      return t("captureTime.changed");
    case "locationSet":
      return t("map.locationSet");
    case "locationRemoved":
      return t("map.locationRemoved");
    case "keywordsAdded":
      return t("keywords.added");
    case "keywordRemoved":
      return t("keywords.removed");
    case "keywordRenamed":
      return t("keywords.renamed");
    case "keywordMoved":
      return t("keywords.moved");
    case "keywordsMerged":
      return t("keywords.merged");
    case "keywordsDeleted":
      return t("keywords.deleted");
    case "keywordExport":
      return t(change.include ? "keywords.exportOn" : "keywords.exportOff");
    case "keywordsImported":
      return t("keywords.imported", counted(change.count));
    case "templateApplied":
      return t("metadataTemplates.applied");
    case "fileMetadataRead":
      return t("fileMetadata.applied");
    case "folderMoved":
      return t("folders.move.done");
    case "folderRenamed":
      return t("fileMove.folderRenamed");
    case "filterPresetCreated":
      return t("filters.presets.created");
    case "filterPresetUpdated":
      return t("filters.presets.updated");
    case "filterPresetRenamed":
      return t("filters.presets.renamed");
    case "filterPresetDeleted":
      return t("filters.presets.deleted");
    case "collectionCreated":
      return t("collections.created");
    case "collectionRenamed":
      return t("collections.renamed");
    case "collectionMoved":
      return t("collections.moved");
    case "collectionDeleted":
      return t("collections.deleted");
    case "collectionMediaAdded":
      return t("collections.mediaAdded");
    case "collectionMediaRemoved":
      return t("collections.mediaRemoved");
    case "quickCollectionAdded":
      return t("collections.quickAdded");
    case "quickCollectionRemoved":
      return t("collections.quickRemoved");
    case "quickCollectionCleared":
      return t("collections.quickCleared");
    case "quickCollectionSaved":
      return t("collections.quickSaved");
    case "smartRulesChanged":
      return t("collections.rulesChanged");
    case "stacked":
      return t("stacks.stacked");
    case "unstacked":
      return t("stacks.unstacked");
    case "stackCoverChanged":
      return t("stacks.coverChanged");
    case "removedFromStack":
      return t("stacks.removed");
    case "pairsStacked":
      return t("stacks.pairsStacked");
    case "burstsStacked":
      return t("stacks.burstsStacked");
    case "fileRelocated":
      return t("relocation.fileRelocated");
    case "folderRelocated":
      return t("relocation.folderRelocated");
    case "mediaMoved":
      return t("relocation.mediaMoved");
    case "mediaRenamed":
      return t("relocation.mediaRenamed");
  }
}

/**
 * Description d'une modification et du nombre d'éléments concernés :
 * « Note : 3 étoiles (12 éléments) ».
 */
export function describeSummary(t: TFunction, summary: EditSummaryDto): string {
  // Une modification de la liste des mots-clés peut ne toucher aucune
  // photo : « (0 élément) » n'apprendrait rien.
  if (summary.count === 0) return describeChange(t, summary.change);
  return t("triage.onItems", {
    description: describeChange(t, summary.change),
    ...counted(summary.count),
  });
}
