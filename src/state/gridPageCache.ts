import type { GridItem } from "../services/library";

/** Nombre d'éléments demandés au cœur en une fois. */
export const GRID_PAGE_SIZE = 200;

/** Charge les éléments `offset` à `offset + limit - 1`. */
export type GridPageLoader = (
  offset: number,
  limit: number,
) => Promise<GridItem[]>;

/**
 * Mémoire des éléments de la grille, chargés page par page à la demande :
 * avec 50 000 éléments, seuls ceux qu'on fait défiler sont demandés au cœur.
 *
 * Après une modification du catalogue (vignettes prêtes, import), les pages
 * déjà chargées restent affichées mais sont marquées « à relire » : elles
 * sont redemandées quand elles sont de nouveau visibles, sans clignotement.
 */
export class GridPageCache {
  private readonly pages = new Map<number, readonly GridItem[]>();
  private readonly indexById = new Map<number, number>();
  /** Pages à relire à leur prochain affichage. */
  private readonly stale = new Set<number>();
  /** Pages en cours de chargement. */
  private readonly loading = new Set<number>();
  /** Pages en échec : pas de nouvel essai avant la prochaine relecture. */
  private readonly failed = new Set<number>();
  /** Incrémenté à chaque relecture : une réponse plus ancienne est ignorée. */
  private generation = 0;
  private isAttached = false;

  /**
   * @param load demande une page au cœur.
   * @param onChange appelé quand des éléments ont changé.
   */
  constructor(
    private readonly load: GridPageLoader,
    private readonly onChange: () => void,
  ) {}

  /** Élément à cette position, s'il est déjà chargé. */
  itemAt(index: number): GridItem | undefined {
    return this.pages.get(Math.floor(index / GRID_PAGE_SIZE))?.[
      index % GRID_PAGE_SIZE
    ];
  }

  /** Position d'un élément déjà chargé. */
  indexOf(mediaId: number): number | undefined {
    return this.indexById.get(mediaId);
  }

  /**
   * Charge les pages nécessaires pour afficher les positions `first` à
   * `last` (incluses) ; sans effet pour celles déjà chargées et à jour.
   */
  ensureRange(first: number, last: number): void {
    if (!this.isAttached || last < first) return;
    const firstPage = Math.floor(Math.max(0, first) / GRID_PAGE_SIZE);
    const lastPage = Math.floor(last / GRID_PAGE_SIZE);
    for (let page = firstPage; page <= lastPage; page += 1) {
      const isUpToDate = this.pages.has(page) && !this.stale.has(page);
      if (!isUpToDate && !this.loading.has(page) && !this.failed.has(page)) {
        this.loadPage(page);
      }
    }
  }

  /** Marque toutes les pages chargées comme « à relire ». */
  invalidate(): void {
    this.generation += 1;
    this.loading.clear();
    this.failed.clear();
    for (const page of this.pages.keys()) this.stale.add(page);
    this.onChange();
  }

  /** Accepte les réponses du cœur (le composant est affiché). */
  attach(): void {
    this.isAttached = true;
  }

  /**
   * Ignore les réponses à venir (le composant a disparu). Les chargements
   * en cours seront refaits si le composant réapparaît.
   */
  detach(): void {
    this.isAttached = false;
    this.generation += 1;
    this.loading.clear();
  }

  private loadPage(page: number): void {
    const generation = this.generation;
    this.loading.add(page);
    this.load(page * GRID_PAGE_SIZE, GRID_PAGE_SIZE).then(
      (items) => {
        if (generation !== this.generation) return;
        this.loading.delete(page);
        this.store(page, items);
        this.onChange();
      },
      (error: unknown) => {
        if (generation !== this.generation) return;
        console.error("Failed to load a page of the grid", error);
        this.loading.delete(page);
        this.failed.add(page);
      },
    );
  }

  private store(page: number, items: readonly GridItem[]): void {
    const offset = page * GRID_PAGE_SIZE;
    // Un élément déplacé vers une page déjà relue garde sa nouvelle position.
    for (const previous of this.pages.get(page) ?? []) {
      const index = this.indexById.get(previous.id);
      if (
        index !== undefined &&
        index >= offset &&
        index < offset + GRID_PAGE_SIZE
      ) {
        this.indexById.delete(previous.id);
      }
    }
    items.forEach((item, position) => {
      this.indexById.set(item.id, offset + position);
    });
    this.pages.set(page, items);
    this.stale.delete(page);
  }
}
