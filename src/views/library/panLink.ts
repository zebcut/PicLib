/** Point d'une photo, de 0 à 1 sur chaque axe (0 : bord gauche ou haut). */
export interface PanFocus {
  x: number;
  y: number;
}

/** Photo qui suit les déplacements des autres. */
export type PanFollower = (focus: PanFocus) => void;

/**
 * Relie le déplacement de plusieurs photos à 100 % (vue Comparaison) : quand
 * on fait glisser l'une, les autres montrent le même point relatif. Le lien
 * vit hors de l'état React : pendant un glissement, seules les positions de
 * défilement changent, sans redessiner les composants.
 */
export class PanLink {
  private focus: PanFocus | null = null;
  private readonly followers = new Set<PanFollower>();

  /**
   * Dernier point montré depuis le dernier changement de zoom : une photo
   * qui apparaît (nouvelle candidate) s'y place directement.
   */
  get current(): PanFocus | null {
    return this.focus;
  }

  /** Oublie le dernier point : un nouveau zoom repart du point cliqué. */
  reset(): void {
    this.focus = null;
  }

  /** Suit les déplacements des autres photos ; renvoie de quoi arrêter. */
  follow(follower: PanFollower): () => void {
    this.followers.add(follower);
    return () => {
      this.followers.delete(follower);
    };
  }

  /** Annonce aux autres photos que `source` montre maintenant `focus`. */
  publish(focus: PanFocus, source: PanFollower): void {
    this.focus = focus;
    for (const follower of this.followers) {
      if (follower !== source) follower(focus);
    }
  }
}
