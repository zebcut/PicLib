import { useTranslation } from "react-i18next";

import { Button } from "../../components/Button";
import type { ImportSourceDto } from "../../services/importSources";
import styles from "./ImportView.module.css";

/**
 * Sources de l'écran d'import : les cartes mémoire et
 * appareils branchés, puis les derniers dossiers choisis (chacun retirable
 * de la liste) ; et le choix de lire aussi les sous-dossiers.
 */
export function ImportSourcePanel({
  cards,
  folders,
  selectedId,
  includeSubfolders,
  onSelect,
  onForget,
  onIncludeSubfolders,
  onChooseFolder,
}: {
  cards: readonly ImportSourceDto[];
  /** Derniers dossiers choisis, du plus récent au plus ancien. */
  folders: readonly ImportSourceDto[];
  selectedId: number | null;
  includeSubfolders: boolean;
  onSelect: (source: ImportSourceDto) => void;
  /** Retire un dossier de la liste (le dossier n'est pas touché). */
  onForget: (source: ImportSourceDto) => void;
  onIncludeSubfolders: (include: boolean) => void;
  onChooseFolder: () => void;
}) {
  const { t } = useTranslation();

  const row = (source: ImportSourceDto, canForget: boolean) => (
    <li key={source.id} className={styles.sourceRow}>
      <button
        type="button"
        className={styles.source}
        aria-current={source.id === selectedId}
        disabled={!source.available}
        title={
          source.available
            ? source.path
            : t("importScreen.unavailable", { path: source.path })
        }
        onClick={() => {
          onSelect(source);
        }}
      >
        {source.name}
      </button>
      {canForget && (
        <button
          type="button"
          className={styles.forget}
          aria-label={t("importScreen.forgetFolder", { name: source.name })}
          title={t("importScreen.forgetFolder", { name: source.name })}
          onClick={() => {
            onForget(source);
          }}
        >
          ×
        </button>
      )}
    </li>
  );

  return (
    <aside className={styles.sidePanel} aria-label={t("importScreen.sources")}>
      <h2 className={styles.heading}>{t("importScreen.sources")}</h2>
      {cards.length > 0 && (
        <>
          <h3 className={styles.panelTitle}>{t("importScreen.cards")}</h3>
          <ul className={styles.sources}>
            {cards.map((card) => row(card, false))}
          </ul>
        </>
      )}
      <h3 className={styles.panelTitle}>{t("importScreen.folders")}</h3>
      {folders.length > 0 ? (
        <ul className={styles.sources}>
          {folders.map((folder) => row(folder, true))}
        </ul>
      ) : (
        <p className={styles.hint}>{t("importScreen.noFolder")}</p>
      )}
      <div className={styles.panelActions}>
        <Button onClick={onChooseFolder}>
          {t("importScreen.chooseFolder")}
        </Button>
        <label className={styles.subfolders}>
          <input
            type="checkbox"
            checked={includeSubfolders}
            onChange={(event) => {
              onIncludeSubfolders(event.target.checked);
            }}
          />
          {t("importScreen.includeSubfolders")}
        </label>
      </div>
    </aside>
  );
}
