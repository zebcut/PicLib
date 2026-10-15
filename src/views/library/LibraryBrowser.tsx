import { useEffect, useMemo, useRef, useState } from "react";
import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import { Modal } from "../../components/Modal";
import { counted } from "../../i18n/format";

import { fetchGridIds } from "../../services/library";
import { chooseRelocatedFile } from "../../services/relocation";
import { toggleStacks } from "../../services/stacks";
import type { GridQuery, GridSort } from "../../services/library";
import { isFiltering } from "../../state/gridFilter";
import type { FilterBarState } from "../../state/gridFilter";
import { ResizeHandle } from "../../components/ResizeHandle";
import { popUpContextMenu } from "../../menu/contextMenu";
import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import { useGridItems } from "../../state/useGridItems";
import type { KeywordListItemDto } from "../../services/keywordList";
import type { KeywordList } from "../../state/useKeywordList";
import { useKeywordPanel } from "../../state/useKeywordPanel";
import type { KeywordSets } from "../../state/useKeywordSets";
import type { MetadataTemplates } from "../../state/useMetadataTemplates";
import { useSelection } from "../../state/useSelection";
import { useStoredBoolean } from "../../state/useStoredBoolean";
import { useStoredNumber } from "../../state/useStoredNumber";
import { useSuggestedKeywords } from "../../state/useSuggestedKeywords";
import { COLUMN_WIDTH } from "./columns";
import type { AutoStackKind, AutoStackRequest } from "./AutoStackDialog";
import { CompareView } from "./CompareView";
import { CullView } from "./CullView";
import { DeletePhotosDialog } from "./DeletePhotosDialog";
import { RenameDialog, initialRenameTemplate } from "./RenameDialog";
import { DeleteRejectedDialog } from "./DeleteRejectedDialog";
import type { EditActions } from "./editActions";
import { FilterBar } from "./FilterBar";
import { Filmstrip } from "./Filmstrip";
import { FilmstripFilter } from "./FilmstripFilter";
import { GridToolbar } from "./GridToolbar";
import { GridView } from "./GridView";
import styles from "./LibraryBrowser.module.css";
import { KeywordListPanel } from "./KeywordListPanel";
import { KeywordPanel } from "./KeywordPanel";
import { LoupeView } from "./LoupeView";
import { MetadataPanel } from "./MetadataPanel";
import { MapView } from "./MapView";
import { fetchLocationPreview } from "../../services/map";
import type { MapArea } from "../../services/map";
import { SurveyView } from "./SurveyView";
import { useExternalEdit } from "./useExternalEdit";
import { useKeywordSetShortcuts } from "./useKeywordSetShortcuts";
import { revealInFinder } from "../../services/externalEditors";
import {
  fetchEditInfo,
  stopFollowing,
  trashUnmodifiedCopy,
} from "../../services/editorWatch";
import type { EditInfoDto } from "../../services/editorWatch";
import { useExternalEditRequests } from "../../state/externalEditRequests";
import { useTriageShortcuts } from "./useTriageShortcuts";

// Les vues et les tests l'importent d'ici, avec le navigateur qui s'en sert.
export type { EditActions } from "./editActions";

/** Liste vide, stable d'un affichage à l'autre. */
const NO_KEYWORDS: readonly KeywordListItemDto[] = [];

/**
 * Vue affichée : la grille, une photo en grand, le mode Tri plein écran, la
 * Comparaison ou l'Ensemble.
 */
type View = "grid" | "loupe" | "cull" | "compare" | "survey" | "map";

/**
 * Vue de base, retrouvée en quittant le mode Tri, la Comparaison ou
 * l'Ensemble.
 */
type BaseView = "grid" | "loupe" | "map";

/** Clé de l'affichage de la filmstrip, retenu d'une session à l'autre. */
const FILMSTRIP_STORAGE_KEY = "piclib.filmstrip.visible";

/** Plus que n'en compte un catalogue : « tous les identifiants ». */
const ALL_IDS = 2 ** 31 - 1;

/**
 * Éléments d'une source (tout le catalogue, dernier import…), filtrés et
 * triés, affichés en
 * Grille, en Loupe, en mode Tri plein écran, en Comparaison ou en
 * Ensemble, avec la sélection qu'elles partagent (les autres vues partent
 * de l'élément actif). La source ne
 * doit pas changer pendant la vie du composant (lui donner une `key`).
 *
 * @param refreshKey les éléments sont relus quand cette valeur change.
 */
export function LibraryBrowser({
  query,
  mapArea,
  onMapAreaChange,
  filterBar,
  isFilterLocked,
  onFilterLockChange,
  isFilterBarVisible,
  onFilterBarChange,
  onSelectionChange,
  onRemoveFromCollection,
  onSortChange,
  refreshKey,
  itemCount,
  cellSize,
  onCellSizeChange,
  triage,
  keywordList,
  keywordSets,
  metadataTemplates,
  onShowKeyword,
  onAutoStack,
}: {
  /** Source, filtre et tri ; la source ne change pas (voir plus haut). */
  query: GridQuery;
  /** Zone de la carte à laquelle la grille est limitée ; `null` : aucune. */
  mapArea: MapArea | null;
  /** Limite la grille à une zone de la carte, ou retire la limite. */
  onMapAreaChange: (area: MapArea | null) => void;
  /** Barre de filtres, affichée au-dessus de la Grille. */
  filterBar: FilterBarState;
  /** Cadenas fermé : le filtre est gardé en changeant de source. */
  isFilterLocked: boolean;
  onFilterLockChange: (isLocked: boolean) => void;
  isFilterBarVisible: boolean;
  onFilterBarChange: (
    update: (state: FilterBarState) => FilterBarState,
  ) => void;
  onSortChange: (sort: GridSort) => void;
  /** Appelé quand la sélection change (panneau Collections). */
  onSelectionChange: (mediaIds: readonly number[]) => void;
  /**
   * Retire des photos de la collection affichée (⌫) ; absent hors d'une
   * collection.
   */
  onRemoveFromCollection?: (mediaIds: number[]) => void;
  refreshKey: number;
  itemCount: number;
  cellSize: number;
  onCellSizeChange: (size: number) => void;
  triage: EditActions;
  keywordList: KeywordList;
  /** Jeux de mots-clés. */
  keywordSets: KeywordSets;
  /** Modèles de métadonnées. */
  metadataTemplates: MetadataTemplates;
  /** Affiche les photos d'un mot-clé (flèche de la liste des mots-clés). */
  onShowKeyword: (keywordId: number) => void;
  /** Ouvre le résumé d'un empilement automatique ; absent : non proposé. */
  onAutoStack?: (request: AutoStackRequest) => void;
}) {
  const { t } = useTranslation();
  // Change quand une pile est repliée ou dépliée (hors de Cmd+Z).
  const [stackLayout, setStackLayout] = useState(0);
  const grid = useGridItems(query, refreshKey + stackLayout);
  const selection = useSelection(grid);
  const isFiltered = isFiltering(query.filter);
  useEffect(() => {
    onSelectionChange([...selection.selection.ids]);
  }, [onSelectionChange, selection.selection.ids]);
  const [view, setView] = useState<View>("grid");
  const [baseView, setBaseView] = useState<BaseView>("grid");
  const [isDeletingRejected, setIsDeletingRejected] = useState(false);
  // Photos à supprimer (Suppr), le temps de la confirmation.
  const [deleting, setDeleting] = useState<readonly number[] | null>(null);
  // Photos à renommer (F2), dans l'ordre de la grille.
  const [renaming, setRenaming] = useState<readonly number[] | null>(null);
  const [isFilmstripVisible, setFilmstripVisible] = useStoredBoolean(
    FILMSTRIP_STORAGE_KEY,
    true,
  );
  const column = COLUMN_WIDTH.right;
  const [columnWidth, setColumnWidth] = useStoredNumber(
    column.storageKey,
    column.default,
    column.min,
    column.max,
  );
  const { activeId, ids: selectedIds } = selection.selection;
  const selectionKeywords = useKeywordPanel(selectedIds, refreshKey);
  const suggestedKeywords = useSuggestedKeywords(activeId, refreshKey);

  /**
   * Garantit un élément actif avant d'afficher une photo en grand : le
   * premier, si rien n'est sélectionné. Faux si la source est vide.
   */
  const ensureActive = (): boolean => {
    if (activeId !== null) return true;
    const first = grid.itemAt(0);
    if (first === undefined) return false;
    selection.selectOnly(first.id);
    return true;
  };

  const openLoupe = () => {
    if (ensureActive()) setView("loupe");
  };

  /**
   * Ouvre le mode Tri, la Comparaison ou l'Ensemble, en retenant la Grille
   * ou la Loupe d'où l'on vient (passer de l'un à l'autre ne la change pas).
   */
  const openOver = (target: Exclude<View, BaseView>) => {
    if (!ensureActive()) return;
    if (view === "grid" || view === "loupe" || view === "map") {
      setBaseView(view);
    }
    setView(target);
  };

  const backToBase = () => {
    setView(baseView);
  };

  /**
   * Rend active une photo parcourue par le mode Tri ou la Comparaison : une
   * sélection parcourue est gardée ; sinon, comme dans la Loupe, la photo
   * devient la seule sélectionnée.
   */
  const showInSequence = (mediaId: number) => {
    if (selectedIds.has(mediaId)) selection.activate(mediaId);
    else selection.selectOnly(mediaId);
  };

  const activeIndex = activeId === null ? undefined : grid.indexOf(activeId);
  const activeItem =
    activeIndex === undefined ? undefined : grid.itemAt(activeIndex);

  // Dernière position connue de l'élément actif : s'il disparaît sous le
  // filtre après une modification, celui qui prend sa place devient actif.
  const lastActiveIndex = useRef<number | null>(null);
  useEffect(() => {
    if (activeIndex !== undefined) lastActiveIndex.current = activeIndex;
  }, [activeIndex]);

  // Un filtre masque des éléments : ils quittent la sélection, pour que
  // rien ne soit modifié sans être visible. Après un changement de filtre,
  // la sélection restante est gardée telle quelle ; après une
  // modification (une photo retirée du filtre par sa nouvelle note), la
  // photo suivante prend la place de celle qui a disparu.
  // Une pile repliée cache de même les photos qu'elle recouvre : après un
  // empilement ou un repli, la sélection est aussi rognée.
  const keepVisible = selection.keepVisible;
  const hasSelection = selectedIds.size > 0;
  const shownQuery = useRef(query);
  const mustPrune = useRef(false);
  useEffect(() => {
    const isNewQuery = shownQuery.current !== query;
    shownQuery.current = query;
    const isStacking = mustPrune.current;
    mustPrune.current = false;
    if ((!isFiltered && !isStacking) || !hasSelection) return;
    let isActive = true;
    fetchGridIds(query, 0, ALL_IDS).then(
      (ids) => {
        if (isActive) {
          keepVisible(ids, isNewQuery ? null : lastActiveIndex.current);
        }
      },
      (error: unknown) => {
        console.error("Failed to read the filtered items", error);
      },
    );
    return () => {
      isActive = false;
    };
  }, [query, refreshKey, stackLayout, isFiltered, hasSelection, keepVisible]);

  // Grille et Loupe : les touches de tri s'appliquent à toute la sélection ;
  // avec l'avance automatique et une seule photo sélectionnée, la suivante
  // devient active (une sélection multiple est gardée telle quelle). Le
  // mode Tri, la Comparaison et l'Ensemble déclarent les leurs par-dessus.
  useTriageShortcuts({
    mediaIds: () => [...selectedIds],
    activeColor: activeItem?.colorLabel,
    apply: triage.apply,
    rotate: triage.rotate,
    onApplied: () => {
      if (!triage.autoAdvance || selectedIds.size > 1) return;
      const next =
        activeIndex === undefined ? undefined : grid.itemAt(activeIndex + 1);
      if (next !== undefined) selection.selectOnly(next.id);
    },
  });

  /** Ajoute à la collection cible, ou en retire (touche B, rond). */
  const toggleInTarget = (mediaIds: number[]) => {
    if (mediaIds.length === 0) return;
    void triage.changeCollections?.({ kind: "toggleInTarget", mediaIds });
  };

  /** Déplie ou replie les piles de ces photos (S, indicateur de pile). */
  const toggleStack = (mediaIds: number[]) => {
    if (mediaIds.length === 0) return;
    toggleStacks(mediaIds).then(
      (collapsed) => {
        if (collapsed === null) return;
        mustPrune.current = collapsed;
        setStackLayout((previous) => previous + 1);
      },
      (error: unknown) => {
        console.error("Failed to expand or collapse the stacks", error);
      },
    );
  };

  // Photo à montrer dès qu'elle apparaît dans la grille (l'original d'une version traitée, caché dans sa pile repliée).
  const pendingShow = useRef<number | null>(null);
  const selectOnly = selection.selectOnly;
  useEffect(() => {
    const id = pendingShow.current;
    if (id !== null && grid.indexOf(id) !== undefined) {
      pendingShow.current = null;
      selectOnly(id);
    }
  }, [grid, selectOnly]);
  /** Montre une photo : la sélectionne, en dépliant au besoin sa pile. */
  const showPhoto = (mediaId: number) => {
    if (grid.indexOf(mediaId) !== undefined) {
      selection.selectOnly(mediaId);
      return;
    }
    if (activeId === null || activeItem?.stack?.collapsed !== true) return;
    pendingShow.current = mediaId;
    toggleStack([activeId]);
  };

  /**
   * Empile ou dépile la sélection (Cmd+G, Cmd+Maj+G) ; la photo active
   * vient en tête : elle donne la couverture.
   */
  const changeStack = (kind: "stack" | "unstack") => {
    if (triage.changeStacks === undefined || selectedIds.size === 0) return;
    const mediaIds =
      activeId === null
        ? [...selectedIds]
        : [activeId, ...[...selectedIds].filter((id) => id !== activeId)];
    // La nouvelle pile est repliée : ses autres photos quittent la sélection.
    mustPrune.current = kind === "stack";
    triage.changeStacks({ kind, mediaIds });
  };

  /**
   * Menu contextuel d'une vignette : empiler la sélection, et,
   * pour une photo d'une pile, la couverture, le retrait, le repli et le
   * dépilement. Une photo hors de la sélection devient la seule
   * sélectionnée, comme dans Lightroom.
   */
  // Photos glissées sur la carte : elles prennent la position et le lieu du
  // point lâché ; celles qui en avaient déjà ne les perdent qu'après
  // confirmation.
  const [locating, setLocating] = useState<{
    mediaIds: number[];
    latitude: number;
    longitude: number;
    overwritten: number;
  } | null>(null);
  const locate = (mediaIds: number[], latitude: number, longitude: number) => {
    const setLocation = triage.setLocation;
    if (setLocation === undefined || mediaIds.length === 0) return;
    fetchLocationPreview(mediaIds).then(
      (preview) => {
        if (preview.overwritten === 0) {
          setLocation(mediaIds, latitude, longitude);
        } else {
          setLocating({
            mediaIds,
            latitude,
            longitude,
            overwritten: preview.overwritten,
          });
        }
      },
      (error: unknown) => {
        console.error("Failed to read the photo positions", error);
      },
    );
  };
  const locateDialog = locating !== null && (
    <Modal
      title={t("map.replaceTitle")}
      actions={
        <>
          <Button
            onClick={() => {
              setLocating(null);
            }}
          >
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            autoFocus
            onClick={() => {
              triage.setLocation?.(
                locating.mediaIds,
                locating.latitude,
                locating.longitude,
              );
              setLocating(null);
            }}
          >
            {t("map.replaceConfirm")}
          </Button>
        </>
      }
    >
      <p>{t("map.replace", counted(locating.overwritten))}</p>
    </Modal>
  );

  /**
   * « Lire depuis le fichier » et « Écraser avec le catalogue », pour les
   * photos `mediaIds` dont le XMP a été modifié par un autre logiciel
   *.
   */
  const xmpConflictEntries = (mediaIds: number[]) => {
    const { readSidecarMetadata, saveMetadataToFiles } = triage;
    if (
      readSidecarMetadata === undefined ||
      saveMetadataToFiles === undefined
    ) {
      return [];
    }
    return [
      {
        text: t("library.xmpReadFromFile"),
        action: () => {
          readSidecarMetadata(mediaIds);
        },
      },
      {
        text: t("library.xmpOverwrite"),
        action: () => {
          saveMetadataToFiles(mediaIds, true);
        },
      },
    ];
  };

  /** Photos visées par un clic sur une photo : la sélection si elle en fait partie. */
  const clickedPhotos = (mediaId: number) => {
    const isSelected = selectedIds.has(mediaId);
    if (!isSelected) selection.selectOnly(mediaId);
    return isSelected
      ? [mediaId, ...[...selectedIds].filter((id) => id !== mediaId)]
      : [mediaId];
  };

  // « Modifier dans… » : Cmd+E, menu Photo, clic droit.
  const externalEdit = useExternalEdit(triage);
  useExternalEditRequests((editorId) => {
    externalEdit.start([...selectedIds], editorId);
  });
  /** Montre le fichier d'une photo dans le Finder (Cmd+R). */
  const reveal = (mediaId: number | null) => {
    if (mediaId === null) return;
    revealInFinder(mediaId).then(
      (result) => {
        if (result.status === "error") {
          if (result.error.kind === "fileMissing") {
            externalEdit.fail(t("externalEdit.revealMissing"));
          } else {
            console.error("Cannot show the file in the Finder", result.error);
          }
        }
      },
      (error: unknown) => {
        console.error("Failed to reach the core", error);
      },
    );
  };
  /** Entrées « Modifier dans… » et « Afficher dans le Finder » d'un clic droit. */
  // Copie `-Modifié` non modifiée à mettre à la corbeille, le temps de la
  // confirmation.
  const [trashingCopy, setTrashingCopy] = useState<{
    mediaId: number;
    name: string;
  } | null>(null);
  const trashCopyDialog = trashingCopy !== null && (
    <Modal
      title={t("externalEdit.trashCopyTitle")}
      actions={
        <>
          <Button
            onClick={() => {
              setTrashingCopy(null);
            }}
          >
            {t("common.cancel")}
          </Button>
          <Button
            variant="primary"
            autoFocus
            onClick={() => {
              const { mediaId } = trashingCopy;
              setTrashingCopy(null);
              selection.selectNone();
              trashUnmodifiedCopy(mediaId).then(
                (started) => {
                  if (!started) {
                    externalEdit.fail(t("externalEdit.trashCopyChanged"));
                  }
                },
                (error: unknown) => {
                  console.error("Failed to trash the copy", error);
                },
              );
            }}
          >
            {t("externalEdit.trashCopyConfirm")}
          </Button>
        </>
      }
    >
      <p>{t("externalEdit.trashCopy", { name: trashingCopy.name })}</p>
    </Modal>
  );

  const editorEntries = (
    mediaIds: number[],
    mediaId: number,
    info: EditInfoDto,
    fileName: string,
  ) => {
    const editors = triage.externalEditors;
    if (triage.editInExternalEditor === undefined || editors === undefined) {
      return [];
    }
    const list = editors?.editors ?? [];
    const defaultEditor = list.find(
      (editor) => editor.id === editors?.defaultId,
    );
    return [
      {
        text:
          defaultEditor === undefined
            ? t("menu.editInNoEditor")
            : t("menu.editIn", { editor: defaultEditor.name }),
        action: () => {
          externalEdit.start(mediaIds, null);
        },
      },
      ...(list.length > 1
        ? [
            {
              text: t("menu.editInSubmenu"),
              items: list.map((editor) => ({
                text: editor.name,
                action: () => {
                  externalEdit.start(mediaIds, editor.id);
                },
              })),
            },
          ]
        : []),
      {
        text: t("menu.revealInFinder"),
        action: () => {
          reveal(mediaId);
        },
      },
      ...(info.followed
        ? [
            {
              text: t("externalEdit.stopFollowing"),
              action: () => {
                stopFollowing(mediaIds).catch((error: unknown) => {
                  console.error("Failed to stop following the photos", error);
                });
              },
            },
          ]
        : []),
      ...(info.unmodifiedCopy
        ? [
            {
              text: t("externalEdit.trashUnmodifiedCopy"),
              action: () => {
                setTrashingCopy({ mediaId, name: fileName });
              },
            },
          ]
        : []),
    ];
  };

  /** Clic sur l'icône « modifié par un autre logiciel » d'une vignette. */
  const openXmpMenu = (mediaId: number) => {
    const entries = xmpConflictEntries(clickedPhotos(mediaId));
    if (entries.length === 0) return;
    popUpContextMenu(entries).catch((error: unknown) => {
      console.error("Failed to open the XMP menu", error);
    });
  };

  /** Lit d'abord ce que la photo propose de plus (suivi, copie), puis ouvre son menu. */
  const openCellMenu = (mediaId: number) => {
    const mediaIds = clickedPhotos(mediaId);
    const none: EditInfoDto = { followed: false, unmodifiedCopy: false };
    const info =
      triage.editInExternalEditor === undefined
        ? Promise.resolve(none)
        : fetchEditInfo(mediaId).catch((error: unknown) => {
            console.error("Failed to read the edit state of the photo", error);
            return none;
          });
    void info.then((found) => {
      popUpCellMenu(mediaId, mediaIds, found);
    });
  };

  const popUpCellMenu = (
    mediaId: number,
    mediaIds: number[],
    info: EditInfoDto,
  ) => {
    const { changeStacks, relocateMedia } = triage;
    const index = grid.indexOf(mediaId);
    const item = index === undefined ? undefined : grid.itemAt(index);
    const stack = item?.stack ?? null;
    const separator = { item: "Separator" } as const;
    const stackEntries =
      changeStacks === undefined
        ? []
        : [
            {
              text: t("menu.stackGroup"),
              enabled: mediaIds.length >= 2,
              action: () => {
                mustPrune.current = true;
                changeStacks({ kind: "stack", mediaIds });
              },
            },
            ...(stack === null
              ? []
              : [
                  {
                    text: t("stacks.setCover"),
                    enabled: stack.position !== 1,
                    action: () => {
                      changeStacks({ kind: "setCover", mediaId });
                    },
                  },
                  {
                    text: t("stacks.removeFromStack"),
                    action: () => {
                      changeStacks({ kind: "remove", mediaIds });
                    },
                  },
                  {
                    text: stack.collapsed
                      ? t("stacks.expand")
                      : t("stacks.collapse"),
                    action: () => {
                      toggleStack([mediaId]);
                    },
                  },
                  {
                    text: t("menu.stackUngroup"),
                    action: () => {
                      changeStacks({ kind: "unstack", mediaIds });
                    },
                  },
                ]),
          ];
    // Une photo manquante se relocalise.
    const fileEntries =
      relocateMedia === undefined || item?.missing !== true
        ? []
        : [
            {
              text: t("relocation.relocateFile"),
              action: () => {
                chooseRelocatedFile(
                  mediaId,
                  t("relocation.fileDialogTitle", { name: item.fileName }),
                  t("relocation.fileFilter"),
                ).then(
                  (path) => {
                    if (path !== null) relocateMedia(mediaId, path);
                  },
                  (error: unknown) => {
                    console.error("Failed to choose the file", error);
                  },
                );
              },
            },
          ];
    const renameEntries =
      triage.renameMedia === undefined
        ? []
        : [
            {
              text: t("rename.rename"),
              action: () => {
                openRename(mediaIds);
              },
            },
          ];
    const removeLocationOf = triage.removeLocation;
    const locationEntries =
      removeLocationOf === undefined
        ? []
        : [
            {
              text: t("map.removeLocation"),
              action: () => {
                removeLocationOf(mediaIds);
              },
            },
          ];
    const xmpEntries =
      item?.xmp === "changedElsewhere" ? xmpConflictEntries(mediaIds) : [];
    const sections = [
      xmpEntries,
      item?.missing === true
        ? []
        : editorEntries(mediaIds, mediaId, info, item?.fileName ?? ""),
      fileEntries,
      renameEntries,
      locationEntries,
      stackEntries,
    ].filter((entries) => entries.length > 0);
    if (sections.length === 0) return;
    popUpContextMenu(
      sections.flatMap((entries, position) =>
        position === 0 ? entries : [separator, ...entries],
      ),
    ).catch((error: unknown) => {
      console.error("Failed to open the photo menu", error);
    });
  };

  /**
   * Ouvre « Renommer » sur ces photos, rangées dans l'ordre de la grille
   * (celui de la séquence).
   */
  const openRename = (mediaIds: number[]) => {
    if (triage.renameMedia === undefined || mediaIds.length === 0) return;
    const position = (id: number) =>
      grid.indexOf(id) ?? Number.MAX_SAFE_INTEGER;
    setRenaming([...mediaIds].sort((a, b) => position(a) - position(b)));
  };

  /** Ouvre le résumé d'un empilement automatique de la sélection. */
  const openAutoStack = (kind: AutoStackKind) => {
    if (onAutoStack === undefined || selectedIds.size === 0) return;
    onAutoStack({ scope: { kind: "media", mediaIds: [...selectedIds] }, kind });
  };

  useShortcuts({
    "stack.group": () => {
      changeStack("stack");
    },
    "stack.ungroup": () => {
      changeStack("unstack");
    },
    "stack.toggle": () => {
      toggleStack([...selectedIds]);
    },
    "stack.autoPairs": () => {
      openAutoStack("pairs");
    },
    "stack.autoBursts": () => {
      openAutoStack("bursts");
    },
    // Respecte l'avance automatique, comme les marqueurs.
    "collection.toggleTarget": () => {
      toggleInTarget([...selectedIds]);
      if (!triage.autoAdvance || selectedIds.size > 1) return;
      const next =
        activeIndex === undefined ? undefined : grid.itemAt(activeIndex + 1);
      if (next !== undefined) selection.selectOnly(next.id);
    },
    "photo.deleteRejected": () => {
      setIsDeletingRejected(true);
    },
    "photo.removeFromCollection": () => {
      if (selectedIds.size === 0) return;
      if (onRemoveFromCollection !== undefined) {
        onRemoveFromCollection([...selectedIds]);
      } else if (triage.removePhotos !== undefined) {
        setDeleting([...selectedIds]);
      }
    },
    "photo.rename": () => {
      openRename([...selectedIds]);
    },
    "photo.editExternal": () => {
      externalEdit.start(
        activeId === null
          ? [...selectedIds]
          : [activeId, ...[...selectedIds].filter((id) => id !== activeId)],
        null,
      );
    },
    "photo.revealInFinder": () => {
      reveal(activeId);
    },
    "photo.delete": () => {
      if (selectedIds.size > 0 && triage.removePhotos !== undefined) {
        setDeleting([...selectedIds]);
      }
    },
    "metadata.readFromFiles": () => {
      triage.readFileMetadata([...selectedIds]);
    },
    "metadata.saveToFiles": () => {
      triage.saveMetadataToFiles?.([...selectedIds]);
    },
    "metadata.removeLocation": () => {
      if (selectedIds.size > 0) triage.removeLocation?.([...selectedIds]);
    },
    "view.grid": () => {
      setView("grid");
    },
    "view.loupe": openLoupe,
    "view.toggleLoupe": () => {
      if (view === "loupe") setView("grid");
      else openLoupe();
    },
    "view.cull": () => {
      if (view === "cull") backToBase();
      else openOver("cull");
    },
    "view.compare": () => {
      if (view === "compare") backToBase();
      else openOver("compare");
    },
    "view.survey": () => {
      if (view === "survey") backToBase();
      else openOver("survey");
    },
    "view.map": () => {
      setView(view === "map" ? "grid" : "map");
    },
    // Vue précédente : celle d'avant le mode Tri, la Comparaison ou
    // l'Ensemble, ou la Grille.
    "view.toggleFilmstrip": () => {
      setFilmstripVisible(!isFilmstripVisible);
    },
    "view.back": () => {
      if (view === "grid" || view === "loupe" || view === "map") {
        setView("grid");
      } else backToBase();
    },
    "selection.all": selection.selectAll,
    // Sans photo active, la Loupe n'a plus rien à montrer : retour franc à
    // la Grille, sinon le clic suivant sur une vignette la rouvrirait.
    "selection.none": () => {
      selection.selectNone();
      setView("grid");
    },
  });

  const keywords =
    keywordList.state.status === "ready"
      ? keywordList.state.keywords
      : NO_KEYWORDS;
  const keywordsById = useMemo(
    () => new Map(keywords.map((keyword) => [keyword.id, keyword])),
    [keywords],
  );
  const toggleKeyword = useKeywordSetShortcuts({
    selectedIds,
    selection: selectionKeywords,
    sets: keywordSets.state,
    keywords,
    add: triage.addKeyword,
    remove: triage.removeKeyword,
  });

  const deleteRejectedDialog = isDeletingRejected && (
    <DeleteRejectedDialog
      source={query.source}
      filter={query.filter}
      onRemove={triage.removeRejected}
      onDone={() => {
        setIsDeletingRejected(false);
        // Les photos sélectionnées ont pu quitter le catalogue.
        selection.selectNone();
        setView("grid");
      }}
      onClose={() => {
        setIsDeletingRejected(false);
      }}
    />
  );

  const renameMedia = triage.renameMedia;
  const renameFirst =
    renaming === null
      ? undefined
      : (() => {
          const index = grid.indexOf(renaming[0] ?? -1);
          return index === undefined ? undefined : grid.itemAt(index);
        })();
  const renameDialog = renaming !== null && renameMedia !== undefined && (
    <RenameDialog
      mediaIds={renaming}
      initialTemplate={initialRenameTemplate(
        renaming.length,
        (renameFirst?.fileName ?? "").replace(/\.[^.]*$/, ""),
      )}
      onRename={renameMedia}
      onClose={() => {
        setRenaming(null);
      }}
    />
  );
  const removePhotos = triage.removePhotos;
  const deletePhotosDialog = deleting !== null &&
    removePhotos !== undefined && (
      <DeletePhotosDialog
        mediaIds={deleting}
        onRemove={removePhotos}
        onDone={() => {
          setDeleting(null);
          // Les photos sélectionnées ont pu quitter le catalogue.
          selection.selectNone();
        }}
        onClose={() => {
          setDeleting(null);
        }}
      />
    );

  if (view === "cull" && activeId !== null) {
    return (
      <>
        <CullView
          grid={grid}
          selectedIds={selectedIds}
          activeId={activeId}
          refreshKey={refreshKey}
          triage={triage}
          onShow={showInSequence}
          onLeave={backToBase}
        />
        {deleteRejectedDialog}
        {deletePhotosDialog}
        {renameDialog}
        {externalEdit.dialog}
        {trashCopyDialog}
      </>
    );
  }
  /** Vue affichée au centre, entre le panneau Catalogue et la colonne de droite. */
  const centerView = () => {
    if (view === "compare" && activeId !== null) {
      return (
        <CompareView
          grid={grid}
          selectedIds={selectedIds}
          activeId={activeId}
          refreshKey={refreshKey}
          triage={triage}
          onShow={showInSequence}
        />
      );
    }
    if (view === "survey" && activeId !== null) {
      return (
        <SurveyView
          grid={grid}
          selectedIds={selectedIds}
          activeId={activeId}
          refreshKey={refreshKey}
          triage={triage}
          onActivate={showInSequence}
        />
      );
    }
    if (view === "map") {
      return (
        <MapView
          source={query.source}
          filter={query.filter}
          refreshKey={refreshKey}
          onSelect={selection.selectIds}
          onLocate={triage.setLocation === undefined ? undefined : locate}
          isLimiting={mapArea !== null}
          onLimitChange={onMapAreaChange}
        />
      );
    }
    if (view === "loupe" && activeId !== null) {
      return (
        <LoupeView
          grid={grid}
          selectedId={activeId}
          onSelect={selection.selectOnly}
        />
      );
    }
    return (
      <>
        {isFilterBarVisible && (
          <FilterBar
            state={filterBar}
            source={query.source}
            refreshKey={refreshKey}
            onChange={onFilterBarChange}
            onChangePreset={triage.changeFilterPreset}
            isLocked={isFilterLocked}
            onLockChange={onFilterLockChange}
          />
        )}
        {mapArea !== null && (
          <div className={styles.areaLimit} role="status">
            <span>{t("map.limited")}</span>
            <Button
              onClick={() => {
                onMapAreaChange(null);
              }}
            >
              {t("map.removeLimit")}
            </Button>
          </div>
        )}
        <GridView
          grid={grid}
          cellSize={cellSize}
          selection={selection}
          onOpen={(mediaId) => {
            selection.selectOnly(mediaId);
            setView("loupe");
          }}
          onDropKeyword={triage.addKeyword}
          onToggleTarget={
            triage.changeCollections === undefined
              ? undefined
              : (mediaId) => {
                  toggleInTarget([mediaId]);
                }
          }
          onToggleStack={(mediaId) => {
            toggleStack([mediaId]);
          }}
          onContextMenu={openCellMenu}
          onXmpConflict={openXmpMenu}
        />
        <GridToolbar
          itemCount={itemCount}
          shownCount={isFiltered ? (grid.count ?? itemCount) : undefined}
          selectedCount={selectedIds.size}
          sort={query.sort}
          onSortChange={onSortChange}
          cellSize={cellSize}
          onCellSizeChange={onCellSizeChange}
        />
      </>
    );
  };

  return (
    <>
      <div className={styles.browser}>
        <div className={styles.center}>
          {centerView()}
          {/* Partout sauf en mode Tri plein écran. */}
          {/* Mini-filtre là où la barre de filtres n'est pas. */}
          {isFilmstripVisible &&
            activeId !== null &&
            (view === "loupe" || view === "compare" || view === "survey") && (
              <FilmstripFilter
                state={filterBar}
                refreshKey={refreshKey}
                onChange={onFilterBarChange}
              />
            )}
          {isFilmstripVisible && (
            <Filmstrip
              grid={grid}
              selection={selection}
              onOpen={(mediaId) => {
                selection.selectOnly(mediaId);
                setView("loupe");
              }}
            />
          )}
        </div>
        <ResizeHandle
          label={t("library.resizeRightColumn")}
          orientation="vertical"
          direction={-1}
          value={columnWidth}
          min={column.min}
          max={column.max}
          defaultValue={column.default}
          onChange={setColumnWidth}
        />
        <aside
          className={styles.rightColumn}
          style={{ width: columnWidth }}
          aria-label={t("library.rightColumn")}
        >
          <KeywordPanel
            selectedIds={selectedIds}
            state={selectionKeywords}
            sets={keywordSets}
            suggestions={suggestedKeywords}
            keywords={keywords}
            onAdd={triage.addKeywords}
            onRemove={triage.removeKeyword}
            onApplyKeyword={(keywordId) => {
              if (selectedIds.size > 0) {
                triage.addKeyword([...selectedIds], keywordId);
              }
            }}
            onToggleKeyword={toggleKeyword}
          />
          <KeywordListPanel
            list={keywordList}
            selectedIds={selectedIds}
            selection={selectionKeywords}
            onAddKeyword={triage.addKeyword}
            onRemoveKeyword={triage.removeKeyword}
            onChange={triage.changeKeywordList}
            onShowKeyword={onShowKeyword}
            onImportList={triage.importKeywordList}
            onExportList={triage.exportKeywordList}
          />
          <MetadataPanel
            activeId={activeId}
            selectedIds={selectedIds}
            refreshKey={refreshKey}
            onSet={triage.setMetadata}
            onChangeCaptureTime={triage.changeCaptureTime}
            templates={metadataTemplates}
            keywordsById={keywordsById}
            onApplyTemplate={triage.applyTemplate}
            onShowPhoto={showPhoto}
          />
        </aside>
        {deleteRejectedDialog}
        {deletePhotosDialog}
        {renameDialog}
        {locateDialog}
        {externalEdit.dialog}
        {trashCopyDialog}
      </div>
    </>
  );
}
