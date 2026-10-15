import { useVirtualizer } from "@tanstack/react-virtual";
import { memo, useRef } from "react";
import { useTranslation } from "react-i18next";

import { formatCaptureTime } from "../../i18n/format";
import { importThumbnailUrl } from "../../services/library";
import type { ImportFileDto } from "../../services/importSources";
import { useElementWidth } from "../../state/useElementWidth";
import styles from "./ImportView.module.css";

/** Largeur souhaitée d'une case, en pixels. */
const CELL_WIDTH = 168;

/** Hauteur d'une case : la vignette, puis le nom et la date. */
const CELL_HEIGHT = 196;

/** Rangées dessinées en plus, au-dessus et au-dessous de la partie visible. */
const OVERSCAN_ROWS = 2;

/**
 * Grille des fichiers d'une source, chacun avec sa case à cocher ; un clic sur une case coche ou décoche aussi l'autre fichier de sa
 * paire. Les fichiers déjà au catalogue et les doublons écartés sont grisés.
 * Seules les rangées visibles sont dessinées, et leurs vignettes demandées.
 */
export function ImportGrid({
  analysisJobId,
  files,
  checked,
  isCheckable,
  onToggle,
}: {
  analysisJobId: number;
  files: readonly ImportFileDto[];
  checked: ReadonlySet<number>;
  isCheckable: (file: ImportFileDto) => boolean;
  onToggle: (file: ImportFileDto) => void;
}) {
  const { t } = useTranslation();
  const scrollRef = useRef<HTMLDivElement>(null);
  const width = useElementWidth(scrollRef);
  const columns = Math.max(1, Math.floor(width / CELL_WIDTH));
  const cellWidth = width > 0 ? width / columns : CELL_WIDTH;
  const rowCount = Math.ceil(files.length / columns);

  // eslint-disable-next-line react-hooks/incompatible-library
  const virtualizer = useVirtualizer({
    count: rowCount,
    getScrollElement: () => scrollRef.current,
    estimateSize: () => CELL_HEIGHT,
    overscan: OVERSCAN_ROWS,
  });

  return (
    <div
      ref={scrollRef}
      className={styles.grid}
      role="list"
      aria-label={t("importScreen.gridLabel")}
    >
      <div
        className={styles.gridContent}
        style={{ height: virtualizer.getTotalSize() }}
      >
        {virtualizer.getVirtualItems().map((row) => (
          <div
            key={row.key}
            className={styles.gridRow}
            style={{
              height: CELL_HEIGHT,
              transform: `translateY(${String(row.start)}px)`,
            }}
          >
            {Array.from({ length: columns }, (_, column) => {
              const file = files[row.index * columns + column];
              if (file === undefined) return null;
              return (
                <ImportCell
                  key={file.index}
                  analysisJobId={analysisJobId}
                  file={file}
                  width={cellWidth}
                  isChecked={checked.has(file.index)}
                  isCheckable={isCheckable(file)}
                  onToggle={onToggle}
                />
              );
            })}
          </div>
        ))}
      </div>
    </div>
  );
}

const ImportCell = memo(function ImportCell({
  analysisJobId,
  file,
  width,
  isChecked,
  isCheckable,
  onToggle,
}: {
  analysisJobId: number;
  file: ImportFileDto;
  width: number;
  isChecked: boolean;
  isCheckable: boolean;
  onToggle: (file: ImportFileDto) => void;
}) {
  const { t } = useTranslation();
  const note =
    file.status === "inCatalog"
      ? t("importScreen.inCatalog")
      : file.status === "duplicate"
        ? t("importScreen.duplicate")
        : null;
  const label = [file.folder, file.name].filter(Boolean).join("/");

  return (
    <label
      role="listitem"
      className={[
        styles.cell,
        isChecked && isCheckable ? styles.checked : "",
        isCheckable ? "" : styles.unavailable,
      ].join(" ")}
      style={{ width }}
      title={label}
    >
      <input
        type="checkbox"
        className={styles.checkbox}
        checked={isChecked && isCheckable}
        disabled={!isCheckable}
        aria-label={file.name}
        onChange={() => {
          onToggle(file);
        }}
      />
      <span className={styles.frame}>
        <img
          className={styles.thumbnail}
          src={importThumbnailUrl(analysisJobId, file.index)}
          alt=""
          loading="lazy"
          draggable={false}
        />
        {(file.isRaw || file.isVideo) && (
          <span className={styles.badge}>
            {file.isVideo ? t("importScreen.video") : "RAW"}
          </span>
        )}
      </span>
      <span className={styles.name}>{file.name}</span>
      <span className={styles.date}>
        {note ??
          (file.dateFromFile
            ? t("importScreen.fileDate", {
                date: formatCaptureTime(file.captureTime),
              })
            : formatCaptureTime(file.captureTime))}
      </span>
    </label>
  );
});
