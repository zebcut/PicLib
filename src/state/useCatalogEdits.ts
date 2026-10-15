import { useCallback, useMemo, useRef, useState } from "react";

import {
  removePhotosFromCatalog,
  removeRejectedFromCatalog,
} from "../services/deletion";
import { changeCollections } from "../services/collections";
import type {
  CollectionOperation,
  CollectionRefusal,
} from "../services/collections";
import { changeFilterPreset } from "../services/filterPresets";
import type {
  FilterPresetOperation,
  FilterPresetRefusal,
} from "../services/filterPresets";
import { redoEdit, rotate, setTriage, undoEdit } from "../services/edits";
import {
  moveMedia,
  moveMediaToNewFolder,
  renameFolder,
  renameMedia,
} from "../services/mediaMove";
import type {
  FileMoveRefusalDto,
  FileMoveResult,
  RenameTemplate,
} from "../services/mediaMove";
import { relocateFolder, relocateMedia } from "../services/relocation";
import type {
  RelocationRefusal,
  RelocationResult,
} from "../services/relocation";
import { changeStacks } from "../services/stacks";
import type { StackOperation, StackRefusal } from "../services/stacks";
import type { GridFilter, GridSource } from "../services/library";
import { changeCaptureTime } from "../services/captureTime";
import { removeLocation, setLocation } from "../services/map";
import { readSidecarMetadata } from "../services/xmp";
import { applyFileMetadata } from "../services/fileMetadata";
import { editInExternalEditor } from "../services/externalEditors";
import type {
  ExternalEditFailure,
  ExternalEditOutcomeDto,
  Outcome,
} from "../services/externalEditors";
import {
  moveFolder,
  removeFolderFromCatalog,
  trashFolder,
} from "../services/folders";
import type {
  FolderMoveResultDto,
  FolderTrashResultDto,
} from "../services/folders";
import type { CaptureTimeChangeDto } from "../services/captureTime";
import { addKeywords, removeKeyword } from "../services/keywords";
import type { AddKeywordsResult } from "../services/keywords";
import { addKeyword, changeKeywordList } from "../services/keywordList";
import type {
  KeywordListOperation,
  KeywordListRefusal,
} from "../services/keywordList";
import { importKeywordList } from "../services/keywordTransfer";
import { applyMetadataTemplate } from "../services/metadataTemplates";
import { setMetadata } from "../services/metadata";
import type { MetadataFieldDto } from "../services/metadata";
import type {
  EditResult,
  EditSummaryDto,
  HistoryState,
  RotationDirectionDto,
  TriageChange,
} from "../services/edits";

/** Dernier événement à confirmer à l'écran. */
export type EditNotice =
  | {
      kind: "done" | "undone" | "redone";
      summary: EditSummaryDto;
      /** Change à chaque événement, même identique au précédent. */
      key: number;
    }
  | { kind: "failed"; key: number }
  /** Information sans modification (photos traitées ajoutées…). */
  | { kind: "info"; message: string; key: number }
  /**
   * Changement refusé (moins de deux photos à empiler, fichier d'un autre
   * format…), à expliquer.
   */
  | { kind: "refused"; refusal: RefusalNotice; key: number };

/** Refus expliqué en bas de la fenêtre. */
export type RefusalNotice =
  | { area: "stacks"; reason: StackRefusal }
  | { area: "relocation"; reason: RelocationRefusal }
  | { area: "move"; reason: FileMoveRefusalDto };

/** Modifications du catalogue et leur historique, pour toute l'application. */
export interface CatalogEdits {
  /** Ce que Cmd+Z et Cmd+Maj+Z feraient (libellés du menu Édition). */
  history: HistoryState;
  /**
   * Change après chaque modification, annulation ou rétablissement réussi :
   * les éléments affichés doivent alors être relus.
   */
  version: number;
  /** Dernier événement à confirmer à l'écran, s'il y en a un. */
  notice: EditNotice | null;
  /** Donne un marqueur, une note ou une couleur aux médias indiqués. */
  applyTriage: (mediaIds: number[], change: TriageChange) => void;
  /** Fait pivoter les médias indiqués d'un quart de tour. */
  rotate: (mediaIds: number[], direction: RotationDirectionDto) => void;
  /** Retire du catalogue les photos rejetées d'une source, filtrées (annulable). */
  removeRejected: (source: GridSource, filter: GridFilter) => void;
  /** Retire du catalogue les photos indiquées (annulable, Suppr). */
  removePhotos: (mediaIds: number[]) => void;
  /**
   * Retire du catalogue un dossier, ses sous-dossiers et leurs médias
   * (annulable).
   */
  removeFolder: (folderId: number) => void;
  /**
   * Place un dossier dans la corbeille ; les éléments affichés sont relus
   * s'il y est parti.
   *
   * @throws si le cœur ne répond pas ou si le catalogue est illisible.
   */
  trashFolder: (folderId: number) => Promise<FolderTrashResultDto>;
  /**
   * Déplace un dossier dans un autre (annulable) ; le refus ou l'échec est
   * renvoyé pour être expliqué.
   *
   * @throws si le cœur ne répond pas ou si le catalogue est illisible.
   */
  moveFolder: (
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
  /** Donne une position aux médias (glissés sur la carte), annulable. */
  setLocation: (
    mediaIds: number[],
    latitude: number,
    longitude: number,
  ) => void;
  /** Retire la position des médias, annulable. */
  removeLocation: (mediaIds: number[]) => void;
  /**
   * « Lire depuis le fichier » : les XMP des médias remplacent ce que le
   * catalogue en sait, annulable.
   */
  readSidecarMetadata: (mediaIds: number[]) => void;
  /**
   * Ajoute aux médias les mots-clés d'une saisie ; renvoie la raison d'un
   * refus (nom ambigu…), à afficher sous le champ, ou `null`.
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
   * l'export » ; renvoie la raison d'un refus (nom déjà pris…), ou `null`.
   */
  changeKeywordList: (
    operation: KeywordListOperation,
  ) => Promise<KeywordListRefusal | null>;
  /** Importe la liste de mots-clés du dernier aperçu (annulable). */
  importKeywords: () => void;
  /**
   * Crée, met à jour, renomme ou supprime un préréglage de filtres
   * (annulable) ; renvoie la raison d'un refus, ou `null`.
   */
  changeFilterPreset: (
    operation: FilterPresetOperation,
  ) => Promise<FilterPresetRefusal | null>;
  /**
   * Crée, renomme, déplace, supprime une collection, ou y ajoute ou en
   * retire des photos (annulable) ; renvoie la raison d'un refus, ou
   * l'identifiant créé (`created`).
   */
  changeCollections: (
    operation: CollectionOperation,
  ) => Promise<
    { refusal: CollectionRefusal } | { refusal: null; createdId: number | null }
  >;
  /**
   * Empile ou dépile des photos (annulable) ; un refus est expliqué par un
   * message en bas de la fenêtre.
   */
  changeStacks: (operation: StackOperation) => void;
  /**
   * Déplace des photos dans un dossier, avec l'autre fichier de leur paire
   * et leurs fichiers annexes (Cmd+Z).
   */
  moveMedia: (mediaIds: number[], folderId: number) => void;
  /** Crée un dossier dans `parentId` et y déplace des photos (Cmd+Z). */
  moveMediaToNewFolder: (
    mediaIds: number[],
    parentId: number,
    name: string,
  ) => void;
  /** Renomme un dossier (Cmd+Z). */
  renameFolder: (folderId: number, name: string) => void;
  /**
   * Renomme des photos avec un modèle, dans l'ordre donné (séquence), avec
   * l'autre fichier de leur paire et leurs fichiers annexes (Cmd+Z).
   */
  renameMedia: (mediaIds: number[], template: RenameTemplate) => void;
  /** Relocalise une photo : son fichier est désormais à `path` (Cmd+Z). */
  relocateMedia: (mediaId: number, path: string) => void;
  /** Relocalise un dossier en `path` (Cmd+Z). */
  relocateFolder: (folderId: number, path: string) => void;
  /** Applique un modèle de métadonnées aux médias indiqués (annulable). */
  applyTemplate: (templateId: number, mediaIds: number[]) => void;
  /**
   * Complète les médias d'après la lecture de leurs fichiers `jobId`
   * (annulable).
   */
  applyFileMetadata: (jobId: number) => void;
  /**
   * Ouvre des photos dans un éditeur externe (`null` : celui de Cmd+E),
   * par des copies `-Modifié` pour celles qui ne sont pas des RAW si
   * `copyNonRaw` ; les copies ajoutées au catalogue font relire la grille
   * (elles ne s'annulent pas : ce sont des imports).
   */
  editInExternalEditor: (
    mediaIds: number[],
    editorId: string | null,
    copyNonRaw: boolean,
  ) => Promise<Outcome<ExternalEditOutcomeDto, ExternalEditFailure>>;
  /** Annule la dernière modification (Cmd+Z). */
  undo: () => void;
  /** Rétablit la dernière modification annulée (Cmd+Maj+Z). */
  redo: () => void;
}

/** Saisie de mots-clés refusée, et pourquoi. */
export type KeywordRefusal = Extract<
  AddKeywordsResult,
  { status: "refused" }
>["failure"];

const EMPTY_HISTORY: HistoryState = { undo: null, redo: null };

/**
 * Suit l'historique des modifications (tenu par le cœur) et relaie les
 * modifications, annulations et rétablissements demandés par l'interface.
 */
export function useCatalogEdits(): CatalogEdits {
  const [history, setHistory] = useState<HistoryState>(EMPTY_HISTORY);
  const [version, setVersion] = useState(0);
  const [notice, setNotice] = useState<EditNotice | null>(null);
  const noticeKey = useRef(0);

  const handle = useCallback(
    (
      request: Promise<EditResult | null>,
      kind: "done" | "undone" | "redone",
    ) => {
      request.then(
        (result) => {
          // `null` : rien n'a été tenté (saisie refusée, signalée ailleurs).
          if (result === null) return;
          noticeKey.current += 1;
          if (result.status === "error") {
            console.error("Failed to modify the catalog", result.failure);
            setNotice({ kind: "failed", key: noticeKey.current });
            return;
          }
          const { summary, history: next } = result.outcome;
          setHistory(next);
          if (summary !== null) {
            setVersion((previous) => previous + 1);
            setNotice({ kind, summary, key: noticeKey.current });
          }
        },
        (error: unknown) => {
          console.error("Failed to reach the core", error);
          noticeKey.current += 1;
          setNotice({ kind: "failed", key: noticeKey.current });
        },
      );
    },
    [],
  );

  const refuse = useCallback((refusal: RefusalNotice) => {
    noticeKey.current += 1;
    setNotice({ kind: "refused", refusal, key: noticeKey.current });
  }, []);
  const relocation = useCallback(
    (request: Promise<RelocationResult>) => {
      handle(
        request.then((relocated) => {
          if (relocated.status === "ok") return relocated.result;
          refuse({ area: "relocation", reason: relocated.refusal });
          return null;
        }),
        "done",
      );
    },
    [handle, refuse],
  );
  const fileMove = useCallback(
    (request: Promise<FileMoveResult>) => {
      handle(
        request.then((moved): EditResult | null => {
          if (moved.status === "error") {
            return { status: "error", failure: { kind: "catalogUnavailable" } };
          }
          const { refusal, failed, outcome } = moved.result;
          if (refusal !== null) {
            refuse({ area: "move", reason: refusal });
            return null;
          }
          if (failed > 0 && outcome === null) {
            return { status: "error", failure: { kind: "filesUnavailable" } };
          }
          return outcome === null ? null : { status: "ok", outcome };
        }),
        "done",
      );
    },
    [handle, refuse],
  );
  const moveMediaTo = useCallback(
    (mediaIds: number[], folderId: number) => {
      fileMove(moveMedia(mediaIds, folderId));
    },
    [fileMove],
  );
  const moveMediaToNew = useCallback(
    (mediaIds: number[], parentId: number, name: string) => {
      fileMove(moveMediaToNewFolder(mediaIds, parentId, name));
    },
    [fileMove],
  );
  const renameMediaWith = useCallback(
    (mediaIds: number[], template: RenameTemplate) => {
      fileMove(renameMedia(mediaIds, template));
    },
    [fileMove],
  );
  const renameFolderTo = useCallback(
    (folderId: number, name: string) => {
      fileMove(renameFolder(folderId, name));
    },
    [fileMove],
  );
  const relocateMediaTo = useCallback(
    (mediaId: number, path: string) => {
      relocation(relocateMedia(mediaId, path));
    },
    [relocation],
  );
  const relocateFolderTo = useCallback(
    (folderId: number, path: string) => {
      relocation(relocateFolder(folderId, path));
    },
    [relocation],
  );

  const applyTriage = useCallback(
    (mediaIds: number[], change: TriageChange) => {
      if (mediaIds.length === 0) return;
      handle(setTriage(mediaIds, change), "done");
    },
    [handle],
  );
  const rotateMedia = useCallback(
    (mediaIds: number[], direction: RotationDirectionDto) => {
      if (mediaIds.length === 0) return;
      handle(rotate(mediaIds, direction), "done");
    },
    [handle],
  );
  const removeRejected = useCallback(
    (source: GridSource, filter: GridFilter) => {
      handle(removeRejectedFromCatalog(source, filter), "done");
    },
    [handle],
  );
  const removePhotos = useCallback(
    (mediaIds: number[]) => {
      if (mediaIds.length === 0) return;
      handle(removePhotosFromCatalog(mediaIds), "done");
    },
    [handle],
  );
  const removeFolder = useCallback(
    (folderId: number) => {
      handle(removeFolderFromCatalog(folderId), "done");
    },
    [handle],
  );
  const trashFolderOf = useCallback(
    async (folderId: number): Promise<FolderTrashResultDto> => {
      const result = await trashFolder(folderId);
      // Pas d'annulation (le dossier se récupère dans la corbeille) : seuls
      // les éléments affichés sont relus.
      if (result.kind === "trashed") setVersion((previous) => previous + 1);
      return result;
    },
    [],
  );
  const moveFolderTo = useCallback(
    async (
      folderId: number,
      destinationId: number,
    ): Promise<FolderMoveResultDto> => {
      const result = await moveFolder(folderId, destinationId);
      if (result.kind === "moved") {
        handle(
          Promise.resolve({ status: "ok", outcome: result.outcome }),
          "done",
        );
      }
      return result;
    },
    [handle],
  );
  const setMetadataOf = useCallback(
    (mediaIds: number[], field: MetadataFieldDto, value: string) => {
      if (mediaIds.length === 0) return;
      handle(setMetadata(mediaIds, field, value), "done");
    },
    [handle],
  );
  const changeCaptureTimeOf = useCallback(
    (mediaIds: number[], change: CaptureTimeChangeDto) => {
      if (mediaIds.length === 0) return;
      handle(changeCaptureTime(mediaIds, change), "done");
    },
    [handle],
  );
  const setLocationOf = useCallback(
    (mediaIds: number[], latitude: number, longitude: number) => {
      if (mediaIds.length === 0) return;
      handle(setLocation(mediaIds, latitude, longitude), "done");
    },
    [handle],
  );
  const readSidecarMetadataOf = useCallback(
    (mediaIds: number[]) => {
      if (mediaIds.length === 0) return;
      handle(readSidecarMetadata(mediaIds), "done");
    },
    [handle],
  );
  const removeLocationOf = useCallback(
    (mediaIds: number[]) => {
      if (mediaIds.length === 0) return;
      handle(removeLocation(mediaIds), "done");
    },
    [handle],
  );
  const addKeywordsTo = useCallback(
    async (
      mediaIds: number[],
      entry: string,
    ): Promise<KeywordRefusal | null> => {
      if (mediaIds.length === 0) return null;
      const request = addKeywords(mediaIds, entry);
      // Un refus est montré sous le champ : pas de message d'échec en plus.
      handle(
        request.then((added) => (added.status === "ok" ? added.result : null)),
        "done",
      );
      try {
        const added = await request;
        return added.status === "refused" ? added.failure : null;
      } catch {
        // Cœur injoignable : déjà signalé par `handle`.
        return null;
      }
    },
    [handle],
  );
  const removeKeywordFrom = useCallback(
    (mediaIds: number[], keywordId: number) => {
      if (mediaIds.length === 0) return;
      handle(removeKeyword(mediaIds, keywordId), "done");
    },
    [handle],
  );
  const addKeywordTo = useCallback(
    (mediaIds: number[], keywordId: number) => {
      if (mediaIds.length === 0) return;
      handle(addKeyword(mediaIds, keywordId), "done");
    },
    [handle],
  );
  const changeList = useCallback(
    async (
      operation: KeywordListOperation,
    ): Promise<KeywordListRefusal | null> => {
      const request = changeKeywordList(operation);
      // Un refus est expliqué par le panneau : pas de message d'échec en
      // plus.
      handle(
        request.then((changed) =>
          changed.status === "ok" ? changed.result : null,
        ),
        "done",
      );
      try {
        const changed = await request;
        return changed.status === "refused" ? changed.refusal : null;
      } catch {
        // Cœur injoignable : déjà signalé par `handle`.
        return null;
      }
    },
    [handle],
  );
  const changePreset = useCallback(
    async (
      operation: FilterPresetOperation,
    ): Promise<FilterPresetRefusal | null> => {
      const request = changeFilterPreset(operation);
      // Un refus est expliqué par la fenêtre du nom : pas de message
      // d'échec en plus.
      handle(
        request.then((changed) =>
          changed.status === "ok" ? changed.result : null,
        ),
        "done",
      );
      try {
        const changed = await request;
        return changed.status === "refused" ? changed.refusal : null;
      } catch {
        // Cœur injoignable : déjà signalé par `handle`.
        return null;
      }
    },
    [handle],
  );
  const changeCollectionTree = useCallback(
    async (
      operation: CollectionOperation,
    ): Promise<
      | { refusal: CollectionRefusal }
      | { refusal: null; createdId: number | null }
    > => {
      const request = changeCollections(operation);
      // Un refus est expliqué par la fenêtre ou le panneau : pas de message
      // d'échec en plus.
      handle(
        request.then((changed) =>
          changed.status === "ok" ? changed.result : null,
        ),
        "done",
      );
      try {
        const changed = await request;
        return changed.status === "refused"
          ? { refusal: changed.refusal }
          : { refusal: null, createdId: changed.createdId };
      } catch {
        // Cœur injoignable : déjà signalé par `handle`.
        return { refusal: null, createdId: null };
      }
    },
    [handle],
  );
  const changeStackGroups = useCallback(
    (operation: StackOperation) => {
      handle(
        changeStacks(operation).then((changed) => {
          if (changed.status === "ok") return changed.result;
          refuse({ area: "stacks", reason: changed.refusal });
          return null;
        }),
        "done",
      );
    },
    [handle, refuse],
  );
  const importKeywords = useCallback(() => {
    handle(importKeywordList(), "done");
  }, [handle]);
  const applyTemplate = useCallback(
    (templateId: number, mediaIds: number[]) => {
      handle(applyMetadataTemplate(templateId, mediaIds), "done");
    },
    [handle],
  );
  const applyFileMetadataOf = useCallback(
    (jobId: number) => {
      handle(applyFileMetadata(jobId), "done");
    },
    [handle],
  );
  const editInEditor = useCallback(
    async (
      mediaIds: number[],
      editorId: string | null,
      copyNonRaw: boolean,
    ) => {
      const result = await editInExternalEditor(mediaIds, editorId, copyNonRaw);
      const copied =
        (result.status === "ok" && result.data.copies > 0) ||
        (result.status === "error" && result.error.kind === "editorNotOpened");
      if (copied) setVersion((previous) => previous + 1);
      return result;
    },
    [],
  );
  const undo = useCallback(() => {
    handle(undoEdit(), "undone");
  }, [handle]);
  const redo = useCallback(() => {
    handle(redoEdit(), "redone");
  }, [handle]);

  return useMemo(
    () => ({
      history,
      version,
      notice,
      applyTriage,
      rotate: rotateMedia,
      removeRejected,
      removePhotos,
      removeFolder,
      trashFolder: trashFolderOf,
      moveFolder: moveFolderTo,
      setMetadata: setMetadataOf,
      changeCaptureTime: changeCaptureTimeOf,
      setLocation: setLocationOf,
      removeLocation: removeLocationOf,
      readSidecarMetadata: readSidecarMetadataOf,
      addKeywords: addKeywordsTo,
      removeKeyword: removeKeywordFrom,
      addKeyword: addKeywordTo,
      changeKeywordList: changeList,
      importKeywords,
      changeFilterPreset: changePreset,
      changeCollections: changeCollectionTree,
      changeStacks: changeStackGroups,
      relocateMedia: relocateMediaTo,
      relocateFolder: relocateFolderTo,
      moveMedia: moveMediaTo,
      moveMediaToNewFolder: moveMediaToNew,
      renameFolder: renameFolderTo,
      renameMedia: renameMediaWith,
      applyTemplate,
      applyFileMetadata: applyFileMetadataOf,
      editInExternalEditor: editInEditor,
      undo,
      redo,
    }),
    [
      history,
      version,
      notice,
      applyTriage,
      rotateMedia,
      removeRejected,
      removePhotos,
      removeFolder,
      trashFolderOf,
      moveFolderTo,
      setMetadataOf,
      changeCaptureTimeOf,
      setLocationOf,
      removeLocationOf,
      readSidecarMetadataOf,
      addKeywordsTo,
      removeKeywordFrom,
      addKeywordTo,
      changeList,
      importKeywords,
      changePreset,
      changeCollectionTree,
      changeStackGroups,
      relocateMediaTo,
      relocateFolderTo,
      moveMediaTo,
      moveMediaToNew,
      renameFolderTo,
      renameMediaWith,
      applyTemplate,
      applyFileMetadataOf,
      editInEditor,
      undo,
      redo,
    ],
  );
}
