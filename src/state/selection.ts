/**
 * Sélection de la bibliothèque : des éléments sélectionnés, dont
 * un est « actif » (celui que la Loupe affiche et dont les panneaux montrent
 * les valeurs). Fonctions pures : chaque opération renvoie une nouvelle
 * sélection.
 *
 * La sélection est gardée par identifiant, pas par position : un import qui
 * décale les éléments de la grille ne la change pas.
 */
export interface Selection {
  /** Éléments sélectionnés. */
  readonly ids: ReadonlySet<number>;
  /** Élément actif, toujours sélectionné ; `null` si rien ne l'est. */
  readonly activeId: number | null;
  /** Position de départ des plages (Maj+clic, Maj+flèches). */
  readonly anchorIndex: number | null;
}

/** Rien de sélectionné. */
export const EMPTY_SELECTION: Selection = {
  ids: new Set(),
  activeId: null,
  anchorIndex: null,
};

/** Sélectionne un seul élément (clic simple, flèches). */
export function selectOnly(id: number, index: number): Selection {
  return { ids: new Set([id]), activeId: id, anchorIndex: index };
}

/**
 * Sélectionne les éléments `ids` (les photos d'un groupe de la carte) ; le
 * premier devient actif. Vide : rien n'est sélectionné.
 */
export function selectMany(
  ids: readonly number[],
  firstIndex: number | null,
): Selection {
  const first = ids[0];
  if (first === undefined)
    return { ids: new Set(), activeId: null, anchorIndex: null };
  return { ids: new Set(ids), activeId: first, anchorIndex: firstIndex };
}

/**
 * Rend actif un élément déjà sélectionné, sans changer la sélection (mode
 * Tri parcourant la sélection) ; un élément non sélectionné est ignoré.
 */
export function activate(
  selection: Selection,
  id: number,
  index: number,
): Selection {
  if (!selection.ids.has(id)) return selection;
  return { ids: selection.ids, activeId: id, anchorIndex: index };
}

/**
 * Ajoute l'élément à la sélection, ou l'en retire s'il y était (Cmd+clic).
 * Un élément ajouté devient actif ; si l'élément actif est retiré, un autre
 * élément sélectionné le devient.
 */
export function toggle(
  selection: Selection,
  id: number,
  index: number,
): Selection {
  const ids = new Set(selection.ids);
  if (!ids.delete(id)) {
    ids.add(id);
    return { ids, activeId: id, anchorIndex: index };
  }
  const activeId =
    selection.activeId === id
      ? (ids.values().next().value ?? null)
      : selection.activeId;
  return { ids, activeId, anchorIndex: index };
}

/**
 * Sélectionne une plage d'éléments (Maj+clic, Maj+flèches) : ceux de
 * `rangeIds`, en plus de la sélection actuelle si `keepExisting` (Cmd+Maj+clic).
 * Le point de départ des plages ne change pas : un second Maj+clic ajuste la
 * même plage.
 */
export function selectRange(
  selection: Selection,
  rangeIds: readonly number[],
  activeId: number,
  keepExisting: boolean,
): Selection {
  const ids = new Set(keepExisting ? selection.ids : []);
  for (const id of rangeIds) ids.add(id);
  ids.add(activeId);
  return { ids, activeId, anchorIndex: selection.anchorIndex };
}

/**
 * Sélectionne tous les éléments (Cmd+A). L'élément actif reste le même s'il
 * en fait partie, sinon c'est le premier.
 */
export function selectAll(
  selection: Selection,
  allIds: readonly number[],
): Selection {
  const ids = new Set(allIds);
  const activeId =
    selection.activeId !== null && ids.has(selection.activeId)
      ? selection.activeId
      : (allIds[0] ?? null);
  return {
    ids,
    activeId,
    anchorIndex: activeId === null ? null : selection.anchorIndex,
  };
}

/**
 * Positions de la première à la dernière d'une plage entre le point de
 * départ et `index`, dans l'ordre croissant.
 */
export function rangeBounds(
  anchorIndex: number | null,
  index: number,
): { first: number; last: number } {
  const anchor = anchorIndex ?? index;
  return { first: Math.min(anchor, index), last: Math.max(anchor, index) };
}

/**
 * Garde de la sélection les seuls éléments encore affichés (`visibleIds`,
 * dans l'ordre de la grille) : un filtre en masque. L'élément actif reste
 * le même s'il est encore affiché, sinon c'est le premier restant. Si plus
 * rien n'est sélectionné et que `fallbackIndex` est donné, l'élément affiché
 * à cette position est sélectionné (la photo qui a pris la place de celle
 * qui a disparu).
 */
export function keepVisible(
  selection: Selection,
  visibleIds: readonly number[],
  fallbackIndex: number | null,
): Selection {
  const kept = visibleIds.filter((id) => selection.ids.has(id));
  if (kept.length === selection.ids.size) return selection;
  if (kept.length === 0) {
    if (fallbackIndex === null || visibleIds.length === 0) {
      return EMPTY_SELECTION;
    }
    const index = Math.min(fallbackIndex, visibleIds.length - 1);
    return selectOnly(visibleIds[index] ?? 0, index);
  }
  const activeId =
    selection.activeId !== null && kept.includes(selection.activeId)
      ? selection.activeId
      : (kept[0] ?? null);
  return {
    ids: new Set(kept),
    activeId,
    anchorIndex: activeId === null ? null : visibleIds.indexOf(activeId),
  };
}
