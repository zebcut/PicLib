import { commands } from "./bindings";
import type { OperationDto, OperationItemDto } from "./bindings";

export type { OperationDto, OperationItemDto };

/**
 * Les dernières opérations sur les fichiers, la plus récente d'abord.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchOperations(): Promise<OperationDto[]> {
  const result = await commands.getOperations();
  if (result.status === "error") {
    throw new Error(`Operations journal unavailable: ${result.error.kind}`);
  }
  return result.data;
}

/**
 * Les premiers fichiers d'une opération.
 *
 * @throws si le cœur ne répond pas ou si le catalogue est illisible.
 */
export async function fetchOperationItems(
  id: number,
): Promise<OperationItemDto[]> {
  const result = await commands.getOperationItems(id);
  if (result.status === "error") {
    throw new Error(`Operation items unavailable: ${result.error.kind}`);
  }
  return result.data;
}
