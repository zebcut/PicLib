/**
 * Dernier élément d'un chemin reçu du cœur (« Vacances » pour
 * « /Users/moi/Images/Vacances »), sous macOS comme sous Windows.
 */
export function lastPathSegment(path: string): string {
  const segments = path.split(/[/\\]/).filter((segment) => segment !== "");
  return segments.at(-1) ?? path;
}
