import process from "node:process";

import react from "@vitejs/plugin-react";
import { defineConfig } from "vite";

// Adresse fournie par la CLI Tauri pour le développement sur appareil
// distant (inutilisée sur le bureau, conservée pour compatibilité).
const host = process.env.TAURI_DEV_HOST;

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],

  // Réglages propres à Tauri, utilisés par `tauri dev` et `tauri build` :
  // 1. ne pas effacer le terminal, pour garder visibles les erreurs Rust ;
  clearScreen: false,
  // 2. Tauri attend un port fixe : échouer plutôt que d'en changer ;
  server: {
    port: 1420,
    strictPort: true,
    host: host ?? false,
    hmr: host ? { protocol: "ws", host, port: 1421 } : undefined,
    // 3. ne pas surveiller le code Rust, recompilé par Tauri lui-même.
    watch: { ignored: ["**/src-tauri/**"] },
  },
});
