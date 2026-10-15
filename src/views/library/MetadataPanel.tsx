import { useId, useRef, useState } from "react";
import type { KeyboardEvent } from "react";
import type { TFunction } from "i18next";
import { useTranslation } from "react-i18next";

import { PanelSection } from "../../components/PanelSection";
import {
  formatAperture,
  formatDimensions,
  formatExposureTime,
  formatFocalLength,
  formatGps,
} from "../../i18n/exif";
import { formatBytes, formatCount, formatDuration } from "../../i18n/format";
import type {
  FieldValueDto,
  GpsDto,
  MediaInfoDto,
  MetadataFieldDto,
} from "../../services/metadata";
import type { CaptureTimeChangeDto } from "../../services/captureTime";
import type { KeywordListItemDto } from "../../services/keywordList";
import { useMetadataPanel } from "../../state/useMetadataPanel";
import type { MetadataTemplates } from "../../state/useMetadataTemplates";
import { CaptureTimeField } from "./CaptureTimeField";
import { MetadataTemplateBar } from "./MetadataTemplateBar";
import styles from "./MetadataPanel.module.css";

/** Clé sous laquelle l'état replié de la section est retenu. */
const SECTION_STORAGE_KEY = "piclib.panel.metadata.open";

/**
 * Panneau Métadonnées : titre, légende, auteur, copyright, lieu
 * et date de prise de vue de la photo active, « < valeurs mixtes > » quand les photos sélectionnées
 * diffèrent ; une saisie validée s'applique à toute la sélection. En
 * dessous, les informations du fichier et de la prise de vue, en lecture
 * seule.
 *
 * @param refreshKey le panneau est relu quand cette valeur change.
 * @param onSet donne une valeur à un champ des médias indiqués.
 * @param onChangeCaptureTime donne une date de prise de vue aux médias
 *   indiqués, ou leur rend leur date d'origine.
 * @param templates modèles de métadonnées(liste « Modèle »).
 * @param keywordsById tous les mots-clés, par identifiant.
 * @param onApplyTemplate applique un modèle aux médias indiqués.
 */
export function MetadataPanel({
  activeId,
  selectedIds,
  refreshKey,
  onSet,
  onChangeCaptureTime,
  templates,
  keywordsById,
  onApplyTemplate,
  onShowPhoto,
}: {
  activeId: number | null;
  selectedIds: ReadonlySet<number>;
  refreshKey: number;
  onSet: (mediaIds: number[], field: MetadataFieldDto, value: string) => void;
  onChangeCaptureTime: (
    mediaIds: number[],
    change: CaptureTimeChangeDto,
  ) => void;
  templates: MetadataTemplates;
  keywordsById: ReadonlyMap<number, KeywordListItemDto>;
  onApplyTemplate: (templateId: number, mediaIds: number[]) => void;
  /** Affiche une photo (l'original d'une version traitée). */
  onShowPhoto?: (mediaId: number) => void;
}) {
  const { t } = useTranslation();
  const state = useMetadataPanel(activeId, selectedIds, refreshKey);

  const content = () => {
    switch (state.status) {
      case "empty":
        return <p className={styles.message}>{t("metadata.noActive")}</p>;
      case "loading":
        return null;
      case "failed":
        return (
          <p className={styles.message} role="alert">
            {t("metadata.loadFailed")}
          </p>
        );
      case "ready":
        return (
          <>
            <div className={styles.fields}>
              {state.panel.fields.map(({ field, value }) => (
                <MetadataTextField
                  // Une nouvelle valeur (après une saisie, une annulation…)
                  // ou une autre photo repart d'un champ neuf.
                  key={`${String(activeId)}:${field}:${valueKey(value)}`}
                  field={field}
                  value={value}
                  onCommit={(text) => {
                    onSet([...selectedIds], field, text);
                  }}
                />
              ))}
              <CaptureTimeField
                key={`${String(activeId)}:captureTime:${valueKey(state.panel.captureTime.value)}`}
                state={state.panel.captureTime}
                originalTime={state.panel.info?.originalCaptureTime ?? null}
                onSet={(local) => {
                  onChangeCaptureTime([...selectedIds], { kind: "set", local });
                }}
                onRestore={() => {
                  onChangeCaptureTime([...selectedIds], { kind: "original" });
                }}
              />
            </div>
            {state.panel.editedFrom !== null && (
              <p className={styles.editedFrom}>
                {state.panel.editedFrom.editor === null
                  ? t("metadata.editedFrom")
                  : t("metadata.editedFromWith", {
                      editor: state.panel.editedFrom.editor,
                    })}{" "}
                <button
                  type="button"
                  className={styles.link}
                  onClick={() => {
                    if (state.panel.editedFrom !== null) {
                      onShowPhoto?.(state.panel.editedFrom.id);
                    }
                  }}
                >
                  {state.panel.editedFrom.fileName}
                </button>
              </p>
            )}
            {state.panel.info !== null && <InfoList info={state.panel.info} />}
          </>
        );
    }
  };

  return (
    <PanelSection title={t("metadata.title")} storageKey={SECTION_STORAGE_KEY}>
      <MetadataTemplateBar
        templates={templates}
        byId={keywordsById}
        selectedIds={selectedIds}
        onApply={onApplyTemplate}
      />
      {content()}
    </PanelSection>
  );
}

/** Identifie une valeur, pour recréer le champ quand elle change. */
function valueKey(value: FieldValueDto): string {
  return value.kind === "mixed" ? "mixed" : `same:${value.value ?? ""}`;
}

/**
 * Champ modifiable. Entrée (ou quitter le champ) valide la saisie, Échap
 * l'abandonne ; dans la légende, Maj+Entrée va à la ligne.
 */
function MetadataTextField({
  field,
  value,
  onCommit,
}: {
  field: MetadataFieldDto;
  value: FieldValueDto;
  onCommit: (text: string) => void;
}) {
  const { t } = useTranslation();
  const id = useId();
  const original = value.kind === "same" ? (value.value ?? "") : "";
  // `null` tant que rien n'a été tapé.
  const [draft, setDraft] = useState<string | null>(null);
  // Échap quitte le champ sans valider : sa sortie ne doit rien enregistrer.
  const isCancelling = useRef(false);
  const isMultiline = field === "caption";

  const commit = () => {
    if (isCancelling.current) {
      isCancelling.current = false;
      return;
    }
    if (draft !== null && draft !== original) onCommit(draft);
  };
  const onKeyDown = (
    event: KeyboardEvent<HTMLInputElement | HTMLTextAreaElement>,
  ) => {
    if (event.key === "Escape") {
      event.preventDefault();
      isCancelling.current = true;
      setDraft(null);
      event.currentTarget.blur();
    } else if (event.key === "Enter" && !(isMultiline && event.shiftKey)) {
      event.preventDefault();
      event.currentTarget.blur();
    }
  };
  const common = {
    id,
    className: styles.input,
    value: draft ?? original,
    placeholder: value.kind === "mixed" ? t("metadata.mixed") : undefined,
    title: t(isMultiline ? "metadata.captionHint" : "metadata.fieldHint"),
    spellCheck: false,
    onChange: (
      event: React.ChangeEvent<HTMLInputElement | HTMLTextAreaElement>,
    ) => {
      setDraft(event.target.value);
    },
    onBlur: commit,
    onKeyDown,
  };

  return (
    <div className={styles.field}>
      <label className={styles.label} htmlFor={id}>
        {t(`metadata.fields.${field}`)}
      </label>
      {isMultiline ? (
        <textarea {...common} rows={3} />
      ) : (
        <input {...common} type="text" />
      )}
    </div>
  );
}

/** Informations en lecture seule de la photo active. */
function InfoList({ info }: { info: MediaInfoDto }) {
  const { t } = useTranslation();
  return (
    <dl className={styles.info}>
      {infoRows(t, info).map(([label, text, title]) => (
        <div key={label} className={styles.row}>
          <dt>{label}</dt>
          <dd title={title ?? text}>{text}</dd>
        </div>
      ))}
    </dl>
  );
}

/**
 * Lignes d'informations connues : [libellé, texte, infobulle éventuelle].
 * Celles que le fichier ne fournit pas sont omises. Les nombres décimaux
 * arrivent du cœur en « nombre ou `null` » (une valeur non représentable
 * y deviendrait `null`) : ils sont vérifiés comme les autres.
 */
function infoRows(
  t: TFunction,
  info: MediaInfoDto,
): [string, string, string?][] {
  const rows: [string, string | null, string?][] = [
    [t("metadata.fileName"), info.fileName],
    [t("metadata.folder"), info.folder],
    [t("metadata.fileSize"), mapNullable(info.fileSize, formatBytes)],
    [
      t("metadata.dimensions"),
      info.width !== null && info.height !== null
        ? formatDimensions(info.width, info.height)
        : null,
    ],
    [t("metadata.duration"), mapNullable(info.durationMs, formatDuration)],
    [t("metadata.camera"), cameraName(info.cameraMake, info.cameraModel)],
    [t("metadata.lens"), info.lens],
    [
      t("metadata.focalLength"),
      mapNullable(info.focalLength, formatFocalLength),
    ],
    [t("metadata.aperture"), mapNullable(info.aperture, formatAperture)],
    [
      t("metadata.exposureTime"),
      mapNullable(info.exposureTime, formatExposureTime),
    ],
    [t("metadata.iso"), mapNullable(info.iso, formatCount)],
    [
      t("metadata.flash"),
      info.flashFired === null
        ? null
        : t(info.flashFired ? "metadata.flashFired" : "metadata.flashNotFired"),
    ],
    [t("metadata.gps"), gpsText(t, info.gps)],
  ];
  return rows.filter(
    (row): row is [string, string, string?] => row[1] !== null,
  );
}

/** Position GPS, si ses deux coordonnées sont connues. */
function gpsText(t: TFunction, gps: GpsDto | null): string | null {
  if (gps === null) return null;
  const { latitude, longitude } = gps;
  return latitude === null || longitude === null
    ? null
    : formatGps(t, latitude, longitude);
}

function mapNullable<T>(
  value: T | null,
  format: (value: T) => string,
): string | null {
  return value === null ? null : format(value);
}

/**
 * Nom de l'appareil : le modèle seul quand il contient déjà la marque
 * (« Canon EOS R6 »), sinon la marque suivie du modèle.
 */
function cameraName(make: string | null, model: string | null): string | null {
  if (model === null) return make;
  if (make === null) return model;
  return model.toLowerCase().startsWith(make.toLowerCase().split(" ")[0] ?? "")
    ? model
    : `${make} ${model}`;
}
