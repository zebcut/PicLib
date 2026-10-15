import { useState } from "react";

import type { PlacementChoice } from "./ImportPlacementFields";
import type {
  BackupChoice,
  ImportPreferencesDto,
} from "../../services/importDestination";
import type {
  ArrangementDto,
  DestinationLayout,
} from "../../services/importSources";
import type { ImportChoices } from "../../services/importFolder";
import type {
  ImportPresetInput,
  ImportPresetSettingsDto,
} from "../../services/importPresets";
import type { RenameTemplate } from "../../services/mediaMove";
import type { MetadataTemplateDto } from "../../services/metadataTemplates";
import { useImportPreferences } from "../../state/useImportPreferences";
import { useImportTemplates } from "../../state/useImportTemplates";
import { DATE_PRESETS } from "./layouts";

/** Réglages de l'écran d'import. */
export interface ImportSettings {
  /** Ne pas importer les doublons. */
  skipDuplicates: boolean;
  /** Modèle de métadonnées posé sur les fichiers ajoutés (`null` : aucun). */
  templateId: number | null;
  /** Laisser les fichiers à leur place, ou les copier. */
  placement: PlacementChoice;
  stackPairs: boolean;
  stackBursts: boolean;
  burstGapSeconds: number;
  /** Rangement des copies. */
  layout: DestinationLayout;
  /** Ajouter un sous-dossier de séance au dernier niveau. */
  useSession: boolean;
  /** Nom du sous-dossier de séance. */
  session: string;
  /** Renommer les copies. */
  useRename: boolean;
  /** Modèle de renommage. */
  renameTemplate: RenameTemplate;
  /** Copier aussi les originaux dans un second emplacement. */
  useBackup: boolean;
  /** Dossier de la sauvegarde, désigné sans chemin. */
  backupChoice: BackupChoice;
  /** Ce dossier, tel qu'il est affiché (vide : pas encore choisi). */
  shownBackup: string;
  /** Lire aussi les sous-dossiers de la source. */
  includeSubfolders: boolean;
}

/** Réglages, et ce qu'il faut pour les afficher. */
export interface ImportSettingsState {
  /** `null` tant que les choix du dernier import ne sont pas lus. */
  settings: ImportSettings | null;
  /** Change certains réglages. */
  update: (changes: Partial<ImportSettings>) => void;
  /** `null` : choix du dernier import illisibles (copie impossible). */
  preferences: ImportPreferencesDto | null;
  /** `null` tant qu'ils ne sont pas lus. */
  templates: readonly MetadataTemplateDto[] | null;
}

/**
 * Réglages de l'écran d'import, présélectionnés d'après le dernier import
 * dès qu'ils sont lus.
 */
export function useImportSettings(): ImportSettingsState {
  const templates = useImportTemplates(true);
  const preferencesState = useImportPreferences(true);
  const preferences =
    preferencesState.status === "ready" ? preferencesState.preferences : null;
  // `null` tant que l'utilisateur n'a rien changé : les choix du dernier
  // import s'appliquent alors.
  const [chosen, setChosen] = useState<ImportSettings | null>(null);
  const defaults =
    templates !== null && preferencesState.status !== "loading"
      ? defaultSettings(preferences, templates)
      : null;
  const settings = chosen ?? defaults;

  return {
    settings,
    update: (changes) => {
      if (settings === null) return;
      setChosen({ ...settings, ...changes });
    },
    preferences,
    templates,
  };
}

function defaultSettings(
  preferences: ImportPreferencesDto | null,
  templates: readonly MetadataTemplateDto[],
): ImportSettings {
  return {
    skipDuplicates: true,
    templateId: templates.find((template) => template.usedAtImport)?.id ?? null,
    placement: {
      mode: preferences?.mode ?? "reference",
      destination: "remembered",
      shownDestination: preferences?.destination ?? "",
      originalPathInCaption: preferences?.originalPathInCaption ?? false,
    },
    stackPairs: preferences?.stackPairs ?? true,
    stackBursts: preferences?.stackBursts ?? false,
    burstGapSeconds: preferences?.burstGapSeconds ?? 2,
    layout: preferences?.layout ?? defaultLayout(),
    useSession: false,
    session: "",
    useRename: preferences?.renameTemplate != null,
    renameTemplate: preferences?.renameTemplate ?? DEFAULT_RENAME,
    useBackup: preferences?.backupDestination != null,
    backupChoice: "remembered",
    shownBackup: preferences?.backupDestination ?? "",
    includeSubfolders: preferences?.includeSubfolders ?? true,
  };
}

/** Modèle de renommage proposé d'abord : `20260914-0001`. */
const DEFAULT_RENAME: RenameTemplate = {
  parts: [
    { kind: "captureDate", format: "compact" },
    { kind: "text", text: "-" },
    { kind: "sequence" },
  ],
  sequenceStart: 1,
  sequenceDigits: 4,
};

/** Modèle de renommage envoyé au cœur (`null` : pas de renommage). */
export function renameOf(settings: ImportSettings): RenameTemplate | null {
  return settings.useRename && settings.renameTemplate.parts.length > 0
    ? settings.renameTemplate
    : null;
}

/** `AAAA/MM/JJ`, le rangement d'avant le choix du rangement. */
function defaultLayout(): DestinationLayout {
  return {
    kind: "date",
    parts: [...(DATE_PRESETS[0]?.parts ?? [])],
  };
}

/** Rangement envoyé au cœur. */
export function arrangementOf(
  settings: ImportSettings,
  months: string[],
): ArrangementDto {
  return {
    layout: settings.layout,
    session:
      settings.useSession && settings.session.trim() !== ""
        ? settings.session
        : null,
    monthNames: months,
  };
}

/**
 * Façon d'importer réellement proposée : copie forcée pour une carte,
 * référence si les réglages de la copie sont illisibles.
 */
export function effectivePlacement(
  placement: PlacementChoice,
  preferences: ImportPreferencesDto | null,
  isCameraCard: boolean,
): PlacementChoice {
  if (preferences === null) return { ...placement, mode: "reference" };
  return isCameraCard ? { ...placement, mode: "copy" } : placement;
}

/**
 * Choix envoyés au cœur pour lancer l'import des fichiers `selected` ;
 * `months` : noms des mois dans la langue de l'interface.
 */
export function importChoices(
  settings: ImportSettings,
  placement: PlacementChoice,
  selected: number[],
  months: string[],
): ImportChoices {
  return {
    skipDuplicates: settings.skipDuplicates,
    templateId: settings.templateId,
    stackPairs: settings.stackPairs,
    stackBursts: settings.stackBursts,
    burstGapSeconds: settings.burstGapSeconds,
    placement:
      placement.mode === "copy"
        ? {
            kind: "copy",
            destination: placement.destination,
            originalPathInCaption: placement.originalPathInCaption,
            arrangement: arrangementOf(settings, months),
            rename: renameOf(settings),
            backup:
              settings.useBackup && settings.shownBackup !== ""
                ? settings.backupChoice
                : null,
          }
        : { kind: "reference" },
    selected,
  };
}

/** Réglages à enregistrer dans un préréglage. */
export function presetInput(
  settings: ImportSettings,
  placement: PlacementChoice,
): ImportPresetInput {
  return {
    mode: placement.mode,
    destination: placement.destination,
    originalPathInCaption: placement.originalPathInCaption,
    layout: settings.layout,
    rename: renameOf(settings),
    backup:
      settings.useBackup && settings.shownBackup !== ""
        ? settings.backupChoice
        : null,
    skipDuplicates: settings.skipDuplicates,
    stackPairs: settings.stackPairs,
    stackBursts: settings.stackBursts,
    burstGapSeconds: settings.burstGapSeconds,
    templateId: settings.templateId,
    includeSubfolders: settings.includeSubfolders,
  };
}

/**
 * Réglages d'un préréglage repris ; un modèle de métadonnées qui n'existe
 * plus est oublié. La séance reste celle en cours.
 */
export function settingsFromPreset(
  current: ImportSettings,
  preset: ImportPresetSettingsDto,
  templates: readonly MetadataTemplateDto[],
): Partial<ImportSettings> {
  return {
    placement: {
      mode: preset.mode,
      destination: preset.destination,
      shownDestination: preset.shownDestination,
      originalPathInCaption: preset.originalPathInCaption,
    },
    layout: preset.layout,
    useRename: preset.rename !== null,
    renameTemplate: preset.rename ?? current.renameTemplate,
    useBackup: preset.shownBackup !== null,
    backupChoice: "chosen",
    shownBackup: preset.shownBackup ?? "",
    skipDuplicates: preset.skipDuplicates,
    stackPairs: preset.stackPairs,
    stackBursts: preset.stackBursts,
    burstGapSeconds: preset.burstGapSeconds,
    templateId: templates.some((template) => template.id === preset.templateId)
      ? preset.templateId
      : null,
    includeSubfolders: preset.includeSubfolders,
  };
}
