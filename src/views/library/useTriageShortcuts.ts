import type {
  ColorLabelDto,
  RotationDirectionDto,
  TriageChange,
} from "../../services/edits";
import { useShortcuts } from "../../shortcuts/ShortcutProvider";
import type { ShortcutHandlers } from "../../shortcuts/ShortcutProvider";
import type { ShortcutId } from "../../shortcuts/registry";

/** Couleurs et leur action du registre des raccourcis. */
const COLOR_SHORTCUTS = [
  ["color.red", "red"],
  ["color.yellow", "yellow"],
  ["color.green", "green"],
  ["color.blue", "blue"],
  ["color.purple", "purple"],
] as const satisfies readonly (readonly [ShortcutId, ColorLabelDto])[];

/** Ce sur quoi portent les touches de tri, et ce qu'elles font. */
export interface TriageTarget {
  /**
   * Médias concernés, lus au moment de l'appui (une sélection de 50 000
   * photos n'est ainsi pas recopiée à chaque affichage).
   */
  mediaIds: () => number[];
  /**
   * Étiquette de couleur de la photo active : la même touche une seconde
   * fois retire la couleur.
   */
  activeColor: ColorLabelDto | null | undefined;
  apply: (mediaIds: number[], change: TriageChange) => void;
  rotate: (mediaIds: number[], direction: RotationDirectionDto) => void;
  /**
   * Appelé après un marqueur, une note ou une couleur, pour l'avance
   * automatique ; jamais après une rotation.
   */
  onApplied: () => void;
}

/**
 * Déclare les touches de tri (P, X, U, 0 à 9, Cmd+[ et Cmd+]) d'une vue,
 * appliquées aux médias qu'elle désigne : la sélection dans la Grille et la
 * Loupe, la photo affichée en mode Tri.
 */
export function useTriageShortcuts(target: TriageTarget): void {
  const applyChange = (change: TriageChange) => {
    const mediaIds = target.mediaIds();
    if (mediaIds.length === 0) return;
    target.apply(mediaIds, change);
    target.onApplied();
  };
  const rotate = (direction: RotationDirectionDto) => {
    const mediaIds = target.mediaIds();
    if (mediaIds.length > 0) target.rotate(mediaIds, direction);
  };

  const handlers: ShortcutHandlers = {
    "flag.picked": () => {
      applyChange({ kind: "flag", flag: "picked" });
    },
    "flag.rejected": () => {
      applyChange({ kind: "flag", flag: "rejected" });
    },
    "flag.none": () => {
      applyChange({ kind: "flag", flag: null });
    },
    "color.none": () => {
      applyChange({ kind: "colorLabel", color: null });
    },
    "photo.rotateLeft": () => {
      rotate("left");
    },
    "photo.rotateRight": () => {
      rotate("right");
    },
  };
  for (const stars of [0, 1, 2, 3, 4, 5] as const) {
    handlers[`rating.${String(stars)}` as ShortcutId] = () => {
      applyChange({ kind: "rating", stars });
    };
  }
  for (const [id, color] of COLOR_SHORTCUTS) {
    handlers[id] = () => {
      applyChange({
        kind: "colorLabel",
        color: target.activeColor === color ? null : color,
      });
    };
  }

  useShortcuts(handlers);
}
