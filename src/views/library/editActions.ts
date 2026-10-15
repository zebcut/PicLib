import type { CatalogEdits } from "../../state/useCatalogEdits";
import type { CaptureTimeChangeDto } from "../../services/captureTime";
import type { RotationDirectionDto, TriageChange } from "../../services/edits";
import type {
  FolderMoveResultDto,
  FolderTrashResultDto,
} from "../../services/folders";
import type {
  KeywordListOperation,
  KeywordListRefusal,
} from "../../services/keywordList";
import type {
  FilterPresetOperation,
  FilterPresetRefusal,
} from "../../services/filterPresets";
import type { GridFilter, GridSource } from "../../services/library";
import type { MetadataFieldDto } from "../../services/metadata";
import type { KeywordRefusal } from "../../state/useCatalogEdits";
import type {
  ExternalEditFailure,
  ExternalEditOutcomeDto,
  ExternalEditorsDto,
  Outcome,
} from "../../services/externalEditors";

/** Modifications du catalogue fournies par l'application. */
export interface EditActions {
  /** Donne un marqueur, une note ou une couleur aux médias indiqués. */
  apply: (mediaIds: number[], change: TriageChange) => void;
  /** Fait pivoter les médias indiqués d'un quart de tour. */
  rotate: (mediaIds: number[], direction: RotationDirectionDto) => void;
  /** Retire du catalogue les photos rejetées d'une source (annulable). */
  removeRejected: (source: GridSource, filter: GridFilter) => void;
  /**
   * Retire du catalogue les photos indiquées (annulable) ; absent, Suppr
   * n'est pas proposé.
   */
  removePhotos?: (mediaIds: number[]) => void;
  /**
   * Retire du catalogue un dossier et ses médias (annulable), ou le place
   * dans la corbeille (panneau Dossiers) ; absents, la suppression d'un
   * dossier n'est pas proposée.
   */
  removeFolder?: (folderId: number) => void;
  trashFolder?: (folderId: number) => Promise<FolderTrashResultDto>;
  /** Déplace un dossier dans un autre (annulable) ; absent : non proposé. */
  moveFolder?: (
    folderId: number,
    destinationId: number,
  ) => Promise<FolderMoveResultDto>;
  /** Donne une valeur à un champ de métadonnées (vide : l'efface). */
  setMetadata: (
    mediaIds: number[],
    field: MetadataFieldDto,
    value: string,
  ) => void;
  /**
   * Donne une date de prise de vue aux médias indiqués, ou leur rend leur
   * date d'origine.
   */
  changeCaptureTime: (mediaIds: number[], change: CaptureTimeChangeDto) => void;
  /**
   * Donne une position aux médias (glissés sur la carte), annulable ;
   * absent : la carte ne reçoit pas de photos.
   */
  setLocation?: (
    mediaIds: number[],
    latitude: number,
    longitude: number,
  ) => void;
  /** Retire la position des médias, annulable ; absent : non proposé. */
  removeLocation?: (mediaIds: number[]) => void;
  /**
   * Ajoute aux médias les mots-clés d'une saisie ; renvoie la raison d'un
   * refus, ou `null`.
   */
  addKeywords: (
    mediaIds: number[],
    entry: string,
  ) => Promise<KeywordRefusal | null>;
  /** Retire un mot-clé des médias. */
  removeKeyword: (mediaIds: number[], keywordId: number) => void;
  /** Pose un mot-clé sur les médias (case cochée de la liste). */
  addKeyword: (mediaIds: number[], keywordId: number) => void;
  /**
   * Renomme, fusionne, supprime un mot-clé ou change « inclure à
   * l'export » ; renvoie la raison d'un refus, ou `null`.
   */
  changeKeywordList: (
    operation: KeywordListOperation,
  ) => Promise<KeywordListRefusal | null>;
  /**
   * Importe ou exporte la liste des mots-clés au format texte de Lightroom
   * (fenêtres de l'application) ; absents, les éléments sont grisés.
   */
  importKeywordList?: () => void;
  /** Change les collections (annulable) ; absent : non proposé. */
  changeCollections?: CatalogEdits["changeCollections"];
  /** Empile ou dépile des photos (annulable) ; absent : non proposé. */
  changeStacks?: CatalogEdits["changeStacks"];
  /** Relocalise une photo (annulable) ; absent : non proposé. */
  relocateMedia?: CatalogEdits["relocateMedia"];
  /** Relocalise un dossier (annulable) ; absent : non proposé. */
  relocateFolder?: CatalogEdits["relocateFolder"];
  /** Déplace des photos dans un dossier (annulable) ; absent : non proposé. */
  moveMedia?: CatalogEdits["moveMedia"];
  /** Crée un dossier avec des photos (annulable) ; absent : non proposé. */
  moveMediaToNewFolder?: CatalogEdits["moveMediaToNewFolder"];
  /** Renomme un dossier (annulable) ; absent : non proposé. */
  renameFolder?: CatalogEdits["renameFolder"];
  /** Renomme des photos (annulable) ; absent : non proposé. */
  renameMedia?: CatalogEdits["renameMedia"];
  /**
   * Crée, met à jour, renomme ou supprime un préréglage de filtres ;
   * absent, les préréglages de l'utilisateur ne sont pas proposés.
   */
  changeFilterPreset?: (
    operation: FilterPresetOperation,
  ) => Promise<FilterPresetRefusal | null>;
  exportKeywordList?: () => void;
  /** Applique un modèle de métadonnées aux médias indiqués (annulable). */
  applyTemplate: (templateId: number, mediaIds: number[]) => void;
  /**
   * Lit les métadonnées des fichiers des médias indiqués, pour compléter
   * ce qui leur manque (après confirmation au-delà d'une photo).
   */
  readFileMetadata: (mediaIds: number[]) => void;
  /**
   * Écrit les métadonnées des médias indiqués dans leurs fichiers XMP
   * ; `overwriteChanged` : remplacer aussi les fichiers modifiés
   * par un autre logiciel.
   */
  saveMetadataToFiles?: (
    mediaIds: number[],
    overwriteChanged?: boolean,
  ) => void;
  /**
   * « Lire depuis le fichier » : les XMP des médias, modifiés par un autre
   * logiciel, remplacent ce que le catalogue en sait (annulable).
   */
  readSidecarMetadata?: (mediaIds: number[]) => void;
  /** Éditeurs externes proposés ; absent : pas de « Modifier dans ». */
  externalEditors?: ExternalEditorsDto | null;
  /**
   * Ouvre des photos dans un éditeur externe (`null` : celui de Cmd+E) ;
   * `copyNonRaw` : par des copies `-Modifié` pour celles qui ne sont pas
   * des RAW.
   */
  editInExternalEditor?: (
    mediaIds: number[],
    editorId: string | null,
    copyNonRaw: boolean,
  ) => Promise<Outcome<ExternalEditOutcomeDto, ExternalEditFailure>>;
  /** Ouvre la fenêtre Préférences (pour ajouter un éditeur). */
  openPreferences?: () => void;
  /** Passer à la photo suivante après chaque action de tri (Grille, Loupe). */
  autoAdvance: boolean;
  /** Même réglage, propre au mode Tri plein écran. */
  cullAutoAdvance: boolean;
}
