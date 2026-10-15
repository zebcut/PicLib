import "./theme/tokens.css";
import "./theme/global.css";

import { StrictMode } from "react";
import { createRoot } from "react-dom/client";

import { App } from "./App";
import { initI18n } from "./i18n";
import { announceInterfaceLanguage } from "./services/places";
import { ShortcutProvider } from "./shortcuts/ShortcutProvider";
import { applyTheme, DEFAULT_THEME } from "./theme/theme";

/** Prépare le thème et les traductions, puis affiche l'interface. */
async function bootstrap(): Promise<void> {
  const rootElement = document.getElementById("root");
  if (rootElement === null) {
    throw new Error("Missing #root element in index.html");
  }

  applyTheme(document.documentElement, DEFAULT_THEME);
  const language = await initI18n(navigator.languages);
  document.documentElement.lang = language;
  // Sans attendre : le cœur remplit alors le lieu des photos en arrière-plan.
  announceInterfaceLanguage(language).catch((error: unknown) => {
    console.error("Failed to announce the interface language", error);
  });

  createRoot(rootElement).render(
    <StrictMode>
      <ShortcutProvider>
        <App />
      </ShortcutProvider>
    </StrictMode>,
  );
}

bootstrap().catch((error: unknown) => {
  console.error("Failed to start the user interface", error);
});
