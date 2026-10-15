import type { ImportFileDto } from "../../services/importSources";

/**
 * Indique si un fichier de la source peut être coché : un fichier déjà au
 * catalogue jamais, un doublon seulement si les doublons ne sont pas
 * écartés.
 */
export function isCheckable(
  file: ImportFileDto,
  skipDuplicates: boolean,
): boolean {
  switch (file.status) {
    case "new":
      return true;
    case "duplicate":
      return !skipDuplicates;
    case "inCatalog":
      return false;
  }
}

/**
 * Fichiers cochés d'emblée : les nouveaux ; avec `onlyNames` (fichiers
 * glissés du Finder), seulement ceux-là parmi les nouveaux, à la racine de
 * la source.
 */
export function initialSelection(
  files: readonly ImportFileDto[],
  onlyNames?: readonly string[],
): Set<number> {
  const only =
    onlyNames === undefined || onlyNames.length === 0
      ? null
      : new Set(onlyNames);
  return new Set(
    files
      .filter(
        (file) =>
          file.status === "new" &&
          (only === null || (file.folder === "" && only.has(file.name))),
      )
      .map((file) => file.index),
  );
}

/** Places des fichiers de chaque groupe (paire RAW + JPEG). */
export function groupMembers(
  files: readonly ImportFileDto[],
): Map<number, number[]> {
  const groups = new Map<number, number[]>();
  for (const file of files) {
    const members = groups.get(file.group);
    if (members === undefined) groups.set(file.group, [file.index]);
    else members.push(file.index);
  }
  return groups;
}

/**
 * Coche ou décoche un fichier avec les autres fichiers cochables de son
 * groupe : une paire RAW + JPEG s'importe d'un bloc.
 */
export function toggleFile(
  selection: ReadonlySet<number>,
  file: ImportFileDto,
  files: readonly ImportFileDto[],
  groups: ReadonlyMap<number, readonly number[]>,
  skipDuplicates: boolean,
): Set<number> {
  const next = new Set(selection);
  const isChecked = selection.has(file.index);
  for (const index of groups.get(file.group) ?? [file.index]) {
    const member = files[index];
    if (member === undefined || !isCheckable(member, skipDuplicates)) continue;
    if (isChecked) next.delete(index);
    else next.add(index);
  }
  return next;
}

/** Tous les fichiers cochables. */
export function selectAll(
  files: readonly ImportFileDto[],
  skipDuplicates: boolean,
): Set<number> {
  return new Set(
    files
      .filter((file) => isCheckable(file, skipDuplicates))
      .map((file) => file.index),
  );
}

/** Fichiers cochés qui peuvent encore l'être, dans l'ordre de la source. */
export function checkedFiles(
  selection: ReadonlySet<number>,
  files: readonly ImportFileDto[],
  skipDuplicates: boolean,
): number[] {
  return files
    .filter(
      (file) => selection.has(file.index) && isCheckable(file, skipDuplicates),
    )
    .map((file) => file.index);
}
