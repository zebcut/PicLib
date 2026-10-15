import { Menu } from "@tauri-apps/api/menu";
import type { MenuOptions } from "@tauri-apps/api/menu";

/**
 * Ouvre un menu contextuel natif (clic droit) sous le pointeur, puis libère
 * ses ressources une fois fermé.
 */
export async function popUpContextMenu(
  items: NonNullable<MenuOptions["items"]>,
): Promise<void> {
  const menu = await Menu.new({ items });
  try {
    await menu.popup();
  } finally {
    await menu.close();
  }
}
