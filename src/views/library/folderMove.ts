import type { FolderDto } from "../../services/folders";

/**
 * Indique si `source` peut être déplacé dans `destination` :
 * un autre dossier du même disque, qui n'est ni son parent actuel, ni lui-même,
 * ni l'un de ses sous-dossiers. Le cœur vérifie de nouveau sur le disque.
 */
export function canMoveInto(
  source: FolderDto,
  destination: FolderDto,
): boolean {
  return (
    source.disk === destination.disk &&
    source.parentId !== destination.id &&
    !isInside(destination.path, source.path)
  );
}

/** `path` est `folder` lui-même ou se trouve dessous. */
function isInside(path: string, folder: string): boolean {
  return path === folder || path.startsWith(`${folder}/`);
}
