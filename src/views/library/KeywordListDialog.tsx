import { useTranslation } from "react-i18next";

import type {
  KeywordListItemDto,
  KeywordListOperation,
  KeywordListRefusal,
} from "../../services/keywordList";
import {
  ChooseKeywordDialog,
  DeleteKeywordsDialog,
  NameDialog,
  listRefusalText,
} from "./KeywordDialogs";

/** Fenêtre ouverte depuis la liste des mots-clés. */
export type KeywordListDialogState =
  | { kind: "create"; parent: KeywordListItemDto | null }
  | { kind: "rename"; keyword: KeywordListItemDto }
  | { kind: "move"; keyword: KeywordListItemDto }
  | { kind: "merge"; keyword: KeywordListItemDto }
  | { kind: "delete"; keyword: KeywordListItemDto }
  | { kind: "deleteUnused" };

/**
 * Fenêtre de la liste des mots-clés : nom (création, renommage), choix
 * d'un autre mot-clé (déplacement, fusion), confirmation de suppression.
 *
 * @param onCreate crée le mot-clé ; renvoie la raison d'un refus, ou
 *   `null`.
 * @param onRun lance une modification annulable ; même retour.
 */
export function KeywordListDialog({
  dialog,
  keywords,
  onCreate,
  onRun,
  onClose,
}: {
  dialog: KeywordListDialogState;
  keywords: readonly KeywordListItemDto[];
  onCreate: (
    parent: KeywordListItemDto | null,
    name: string,
    includeOnExport: boolean,
  ) => Promise<KeywordListRefusal | null>;
  onRun: (
    operation: KeywordListOperation,
  ) => Promise<KeywordListRefusal | null>;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  switch (dialog.kind) {
    case "create":
      return (
        <NameDialog
          title={
            dialog.parent === null
              ? t("keywordList.createRootTitle")
              : t("keywordList.createInsideTitle", {
                  name: dialog.parent.name,
                })
          }
          initialName=""
          askExport
          onSubmit={(name, includeOnExport) =>
            onCreate(dialog.parent, name, includeOnExport)
          }
          refusalText={(refusal) => listRefusalText(t, refusal)}
          onClose={onClose}
        />
      );
    case "rename":
      return (
        <NameDialog
          title={t("keywordList.renameTitle", { name: dialog.keyword.name })}
          initialName={dialog.keyword.name}
          askExport={false}
          onSubmit={(name) =>
            onRun({ kind: "rename", keywordId: dialog.keyword.id, name })
          }
          refusalText={(refusal) => listRefusalText(t, refusal)}
          onClose={onClose}
        />
      );
    case "move":
      return (
        <ChooseKeywordDialog
          keyword={dialog.keyword}
          keywords={keywords}
          title={t("keywordList.moveTitle", { name: dialog.keyword.name })}
          explanation={t("keywordList.moveExplanation", {
            name: dialog.keyword.name,
          })}
          actionLabel={t("keywordList.move")}
          offerRoot
          onChoose={(parentId) =>
            onRun({ kind: "move", keywordId: dialog.keyword.id, parentId })
          }
          onClose={onClose}
        />
      );
    case "merge":
      return (
        <ChooseKeywordDialog
          keyword={dialog.keyword}
          keywords={keywords}
          title={t("keywordList.mergeTitle", { name: dialog.keyword.name })}
          explanation={t("keywordList.mergeExplanation", {
            name: dialog.keyword.name,
          })}
          actionLabel={t("keywordList.merge")}
          offerRoot={false}
          onChoose={(intoId) =>
            intoId === null
              ? Promise.resolve(null)
              : onRun({ kind: "merge", keywordId: dialog.keyword.id, intoId })
          }
          onClose={onClose}
        />
      );
    case "delete":
      return (
        <DeleteKeywordsDialog
          title={t("keywordList.deleteTitle", { name: dialog.keyword.name })}
          keywordCount={branchSize(keywords, dialog.keyword.id)}
          photoCount={dialog.keyword.photoCount}
          onDelete={() => {
            void onRun({ kind: "delete", keywordId: dialog.keyword.id });
          }}
          onClose={onClose}
        />
      );
    case "deleteUnused":
      return (
        <DeleteKeywordsDialog
          title={t("keywordList.deleteUnusedTitle")}
          keywordCount={
            keywords.filter((keyword) => keyword.photoCount === 0).length
          }
          photoCount={0}
          onDelete={() => {
            void onRun({ kind: "deleteUnused" });
          }}
          onClose={onClose}
        />
      );
  }
}

/** Nombre de mots-clés de la branche de `rootId`, lui compris. */
function branchSize(
  keywords: readonly KeywordListItemDto[],
  rootId: number,
): number {
  const children = new Map<number, number[]>();
  for (const keyword of keywords) {
    if (keyword.parentId === null) continue;
    const siblings = children.get(keyword.parentId) ?? [];
    siblings.push(keyword.id);
    children.set(keyword.parentId, siblings);
  }
  const seen = new Set<number>();
  const pending = [rootId];
  while (pending.length > 0) {
    const id = pending.pop();
    if (id === undefined || seen.has(id)) continue;
    seen.add(id);
    pending.push(...(children.get(id) ?? []));
  }
  return seen.size;
}
