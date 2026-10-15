import { useTranslation } from "react-i18next";

import { applyPreset } from "../../state/filterPresets";
import { changeFilter } from "../../state/gridFilter";
import type { FilterBarState } from "../../state/gridFilter";
import { useStoredColumns } from "../../state/useStoredColumns";
import { AttributeFilter } from "./FilterBar";
import type { FilterBarUpdate } from "./FilterBar";
import styles from "./FilterBar.module.css";
import { PresetMenu } from "./PresetMenu";

/**
 * Mini-filtre au-dessus de la filmstrip, en Loupe, Comparaison
 * et Ensemble : marqueur, note, couleur et préréglages. C'est le même
 * filtre que la barre de filtres : un critère réglé ici ouvre son onglet
 * Attribut. Les préréglages s'enregistrent depuis la barre.
 *
 * @param refreshKey les préréglages sont relus quand cette valeur change.
 */
export function FilmstripFilter({
  state,
  refreshKey,
  onChange,
}: {
  state: FilterBarState;
  refreshKey: number;
  onChange: FilterBarUpdate;
}) {
  const { t } = useTranslation();
  const [columns, setColumns] = useStoredColumns();

  return (
    <div className={styles.mini} role="group" aria-label={t("filters.mini")}>
      <AttributeFilter
        filter={state.filter}
        withKinds={false}
        className={styles.miniAttributes}
        onChange={(patch) => {
          onChange((current) =>
            changeFilter(
              {
                ...current,
                tabs: current.tabs.includes("attribute")
                  ? current.tabs
                  : [...current.tabs, "attribute"],
              },
              { ...current.filter, ...patch },
            ),
          );
        }}
      />
      <PresetMenu
        state={state}
        columns={columns}
        refreshKey={refreshKey}
        onApply={(content) => {
          onChange(() => applyPreset(content));
          if (content.tabs.includes("metadata")) setColumns(content.columns);
        }}
      />
    </div>
  );
}
