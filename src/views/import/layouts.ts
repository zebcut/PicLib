import type { DatePart, DestinationLayout } from "../../services/importSources";

/** Rangement par date prêt à l'emploi. */
export interface DatePreset {
  id: string;
  /** Modèle tel qu'affiché. */
  label: string;
  parts: DatePart[];
}

const level: DatePart = { kind: "level" };
const dash: DatePart = { kind: "text", text: "-" };

/** Modèles par date proposés, dans l'ordre du menu. */
export const DATE_PRESETS: readonly DatePreset[] = [
  {
    id: "yearMonthDay",
    label: "AAAA/MM/JJ",
    parts: [{ kind: "year" }, level, { kind: "month" }, level, { kind: "day" }],
  },
  {
    id: "yearThenDate",
    label: "AAAA/AAAA-MM-JJ",
    parts: [
      { kind: "year" },
      level,
      { kind: "year" },
      dash,
      { kind: "month" },
      dash,
      { kind: "day" },
    ],
  },
  {
    id: "yearMonth",
    label: "AAAA/MM",
    parts: [{ kind: "year" }, level, { kind: "month" }],
  },
  {
    id: "yearMonthNameDay",
    label: "AAAA/MM - Mois/JJ",
    parts: [
      { kind: "year" },
      level,
      { kind: "month" },
      { kind: "text", text: " - " },
      { kind: "monthName" },
      level,
      { kind: "day" },
    ],
  },
  {
    id: "date",
    label: "AAAA-MM-JJ",
    parts: [{ kind: "year" }, dash, { kind: "month" }, dash, { kind: "day" }],
  },
];

/** Choix du menu de rangement. */
export type LayoutChoice =
  `preset:${string}` | "customDate" | "singleFolder" | "sourceTree";

function sameParts(a: readonly DatePart[], b: readonly DatePart[]): boolean {
  return JSON.stringify(a) === JSON.stringify(b);
}

/** Choix du menu qui correspond à un rangement. */
export function layoutChoice(layout: DestinationLayout): LayoutChoice {
  switch (layout.kind) {
    case "singleFolder":
      return "singleFolder";
    case "sourceTree":
      return "sourceTree";
    case "date": {
      const preset = DATE_PRESETS.find((candidate) =>
        sameParts(candidate.parts, layout.parts),
      );
      return preset === undefined ? "customDate" : `preset:${preset.id}`;
    }
  }
}

/**
 * Rangement d'un choix du menu ; un modèle personnalisé part du rangement
 * actuel s'il est par date.
 */
export function layoutFor(
  choice: LayoutChoice,
  current: DestinationLayout,
): DestinationLayout {
  switch (choice) {
    case "singleFolder":
      return { kind: "singleFolder", name: "" };
    case "sourceTree":
      return { kind: "sourceTree" };
    case "customDate":
      return current.kind === "date"
        ? current
        : { kind: "date", parts: [...(DATE_PRESETS[0]?.parts ?? [])] };
    default: {
      const id = choice.slice("preset:".length);
      const preset = DATE_PRESETS.find((candidate) => candidate.id === id);
      return { kind: "date", parts: [...(preset?.parts ?? [])] };
    }
  }
}

/**
 * Noms des douze mois dans la langue de l'interface, avec une majuscule
 * (« Septembre »), pour le rangement « AAAA/MM - Mois/JJ ».
 */
export function monthNames(language: string): string[] {
  const format = new Intl.DateTimeFormat(language, {
    month: "long",
    timeZone: "UTC",
  });
  return Array.from({ length: 12 }, (_, month) => {
    const name = format.format(new Date(Date.UTC(2000, month, 1)));
    return name.charAt(0).toLocaleUpperCase(language) + name.slice(1);
  });
}
