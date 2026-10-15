/**
 * Arbres affichés en liste(mots-clés) : construction à
 * partir d'éléments « à plat » (chacun connaît son parent), filtre par
 * recherche, et lignes visibles selon les branches dépliées.
 */

/** Élément qui connaît son parent (`null` : à la racine). */
export interface TreeItem {
  id: number;
  parentId: number | null;
  name: string;
}

/** Nœud de l'arbre : un élément et ses enfants, triés par nom. */
export interface TreeNode<T extends TreeItem> {
  item: T;
  children: TreeNode<T>[];
  /**
   * Clé de recherche du nom (`searchKey`), calculée une seule fois : une
   * recherche parcourt des dizaines de milliers de noms à chaque lettre.
   */
  key: string;
}

/** Ligne affichée : un nœud, sa profondeur et son état. */
export interface TreeRow<T extends TreeItem> {
  node: TreeNode<T>;
  /** 0 pour la racine. */
  depth: number;
  hasChildren: boolean;
  isExpanded: boolean;
  /** Parent affiché au-dessus, pour ← au clavier. */
  parentId: number | null;
  /** Rang parmi ses frères, à partir de 1 (pour les lecteurs d'écran). */
  position: number;
  /** Nombre de frères, lui compris. */
  siblingCount: number;
}

/**
 * Clé de recherche d'un nom : minuscules, sans accents (« ete » trouve
 * « Été »), comme l'autocomplétion du cœur.
 */
export function searchKey(text: string): string {
  return text.normalize("NFD").replace(/\p{M}/gu, "").toLowerCase();
}

/**
 * Chemin d'un élément, du plus haut parent à lui (« Lieux > Paris ») ; vide
 * s'il est inconnu. Protégé contre une boucle dans des données abîmées.
 */
export function itemPath(
  byId: ReadonlyMap<number, TreeItem>,
  id: number,
): string {
  const names: string[] = [];
  let current = byId.get(id);
  while (current !== undefined && names.length <= byId.size) {
    names.unshift(current.name);
    current =
      current.parentId === null ? undefined : byId.get(current.parentId);
  }
  return names.join(" > ");
}

/**
 * Construit l'arbre, chaque niveau trié par nom (sans accents ni
 * majuscules). Un élément dont le parent est absent est placé à la racine ;
 * un élément pris dans une boucle (base abîmée) n'apparaît pas.
 */
export function buildTree<T extends TreeItem>(
  items: readonly T[],
): TreeNode<T>[] {
  const nodes = new Map<number, TreeNode<T>>();
  for (const item of items) {
    nodes.set(item.id, { item, children: [], key: searchKey(item.name) });
  }
  const roots: TreeNode<T>[] = [];
  for (const node of nodes.values()) {
    const parentId = node.item.parentId;
    const parent = parentId === null ? undefined : nodes.get(parentId);
    if (parent === undefined) roots.push(node);
    else parent.children.push(node);
  }
  const collator = (a: TreeNode<T>, b: TreeNode<T>) =>
    compare(a.key, b.key) || compare(a.item.name, b.item.name);
  const sort = (level: TreeNode<T>[]) => {
    level.sort(collator);
    for (const node of level) sort(node.children);
  };
  sort(roots);
  return roots;
}

/**
 * Garde les nœuds dont le nom contient `query` (sans accents ni
 * majuscules), avec leurs parents pour les situer ; un nœud trouvé garde
 * aussi tous ses descendants. Une recherche vide garde tout.
 */
export function filterTree<T extends TreeItem>(
  roots: readonly TreeNode<T>[],
  query: string,
): TreeNode<T>[] {
  const key = searchKey(query.trim());
  if (key === "") return [...roots];
  const filter = (level: readonly TreeNode<T>[]): TreeNode<T>[] =>
    level.flatMap((node) => {
      if (node.key.includes(key)) return [node];
      const children = filter(node.children);
      return children.length === 0 ? [] : [{ ...node, children }];
    });
  return filter(roots);
}

/**
 * Lignes à afficher : parcours en profondeur, sans descendre dans les
 * branches repliées.
 */
export function visibleRows<T extends TreeItem>(
  roots: readonly TreeNode<T>[],
  isExpanded: (id: number) => boolean,
): TreeRow<T>[] {
  const rows: TreeRow<T>[] = [];
  const visit = (
    level: readonly TreeNode<T>[],
    depth: number,
    parentId: number | null,
  ) => {
    for (const [index, node] of level.entries()) {
      const hasChildren = node.children.length > 0;
      const expanded = hasChildren && isExpanded(node.item.id);
      rows.push({
        node,
        depth,
        hasChildren,
        isExpanded: expanded,
        parentId,
        position: index + 1,
        siblingCount: level.length,
      });
      if (expanded) visit(node.children, depth + 1, node.item.id);
    }
  };
  visit(roots, 0, null);
  return rows;
}

/**
 * Branches d'un nœud : lui et tous ses descendants qui ont des enfants, pour
 * tout déplier ou tout replier d'un coup (Option+clic sur le triangle).
 */
export function branchIds<T extends TreeItem>(node: TreeNode<T>): number[] {
  const ids: number[] = [];
  const stack = [node];
  for (let next = stack.pop(); next !== undefined; next = stack.pop()) {
    if (next.children.length === 0) continue;
    ids.push(next.item.id);
    // Une boucle plutôt que `push(...children)` : une racine peut avoir des
    // dizaines de milliers d'enfants.
    for (const child of next.children) stack.push(child);
  }
  return ids;
}

function compare(a: string, b: string): number {
  if (a < b) return -1;
  return a > b ? 1 : 0;
}
