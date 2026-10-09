/**
 * Owns one settings area's draft and its save flow.
 *
 * The draft is the single writer of pending edits: user input, Discard,
 * save success and conflict resolution are the only things that change it.
 * A save that meets a stale `If-Match` reconciles once against the server's
 * newer values and otherwise hands the user a conflict to resolve.
 */
import { useMutation, useQueryClient } from "@tanstack/react-query";
import { useState } from "react";

import { ApiError, isIfMatchMismatch, isIfMatchRequired } from "@/api/errors";
import {
  updateSettings,
  type SettingsKey,
  type SettingsPatch,
  type SettingsSnapshot,
  type SettingsValues,
} from "@/api/settings";
import { invokeUnauthenticatedHandler } from "@/lib/query/client";
import { queryKeys } from "@/lib/query/keys";

import {
  EMPTY_DRAFT,
  buildPatch,
  classify,
  describePatched,
  dirtyKeys,
  dropKeys,
  elsewhereSummary,
  fieldForServerDetail,
  patchKeys,
  setBoolean,
  setEpub,
  setText,
  withoutKeys,
  type Draft,
} from "./editor";
import {
  ALL_KEYS,
  areaOfKey,
  deletionCopy,
  describeValue,
  omitKey,
  type AreaId,
  type BooleanKey,
  type NumberKey,
} from "./fields";
import { settingsQueryOptions } from "./queries";

export type ConflictChoice = "mine" | "theirs";

export type ConflictRow = {
  key: SettingsKey;
  mine: string;
  theirs: string;
  choice: ConflictChoice;
};

export type Notice =
  | { kind: "none" }
  | {
      kind: "saved";
      at: Date;
      reapplied: boolean;
      elsewhere: string | null;
      restartKeys: SettingsKey[];
    }
  | { kind: "reapplying" }
  | { kind: "validation"; errors: Partial<Record<SettingsKey, string>>; unmapped: string | null }
  | { kind: "conflict"; rows: ConflictRow[] }
  | { kind: "forbidden" }
  | { kind: "failed"; reason: "network" | "server" };

export type Elsewhere = Partial<Record<SettingsKey, string>>;

export type SettingsEditor = {
  draft: Draft;
  dirty: SettingsKey[];
  notice: Notice;
  elsewhere: Elsewhere;
  phase: "idle" | "saving" | "reapplying";
  pendingConfirm: BooleanKey | null;
  editBoolean: (key: BooleanKey, value: boolean) => void;
  editText: (key: NumberKey, text: string) => void;
  editEpub: (value: boolean) => void;
  confirmDeletion: () => void;
  cancelDeletion: () => void;
  discard: () => void;
  save: () => void;
  choose: (key: SettingsKey, choice: ConflictChoice) => void;
  saveChoices: () => void;
  discardAndReload: () => void;
};

type Outcome =
  | {
      kind: "saved";
      written: SettingsSnapshot;
      reapplied: boolean;
      changed: SettingsKey[];
      base: SettingsValues;
      restart: boolean;
    }
  | { kind: "agreed"; fresh: SettingsSnapshot; base: SettingsValues; changed: SettingsKey[] }
  | {
      kind: "conflict";
      fresh: SettingsSnapshot;
      base: SettingsValues;
      patch: SettingsPatch;
      overlap: SettingsKey[];
      changed: SettingsKey[];
    };

type SaveArgs = { base: SettingsSnapshot; patch: SettingsPatch };

function isPrecondition(err: unknown): boolean {
  return isIfMatchMismatch(err) || isIfMatchRequired(err);
}

function merged(
  base: SettingsSnapshot,
  written: { values: SettingsValues; revision: number; updatedAt: string },
): SettingsSnapshot {
  return {
    ...base,
    values: written.values,
    revision: written.revision,
    updatedAt: written.updatedAt,
  };
}

export function useSettingsEditor(area: AreaId, base: SettingsSnapshot): SettingsEditor {
  const queryClient = useQueryClient();
  const [draft, setDraft] = useState<Draft>(EMPTY_DRAFT);
  const [notice, setNotice] = useState<Notice>({ kind: "none" });
  const [elsewhere, setElsewhere] = useState<Elsewhere>({});
  const [pendingConfirm, setPendingConfirm] = useState<BooleanKey | null>(null);

  const dirty = dirtyKeys(area, draft, base.values);

  async function refetch(): Promise<SettingsSnapshot> {
    return queryClient.query({ ...settingsQueryOptions(), staleTime: 0 });
  }

  async function runSave({ base: start, patch }: SaveArgs): Promise<Outcome> {
    try {
      const written = await updateSettings(patch);
      return {
        kind: "saved",
        written: merged(start, written),
        reapplied: false,
        changed: [],
        base: start.values,
        restart: written.restartRequired,
      };
    } catch (err) {
      if (!isPrecondition(err)) throw err;
    }

    setNotice({ kind: "reapplying" });
    const fresh = await refetch();
    const found = classify(start.values, fresh.values, patch);
    if (found.overlap.length > 0) {
      return {
        kind: "conflict",
        fresh,
        base: start.values,
        patch,
        overlap: found.overlap,
        changed: found.changedByOthers,
      };
    }
    const remaining = withoutKeys(patch, found.agreed);
    if (patchKeys(remaining).length === 0) {
      return { kind: "agreed", fresh, base: start.values, changed: found.changedByOthers };
    }
    try {
      const written = await updateSettings(remaining);
      return {
        kind: "saved",
        written: merged(fresh, written),
        reapplied: true,
        changed: found.changedByOthers,
        base: start.values,
        restart: written.restartRequired,
      };
    } catch (err) {
      if (!isPrecondition(err)) throw err;
    }

    const latest = await refetch();
    const again = classify(start.values, latest.values, patch);
    return {
      kind: "conflict",
      fresh: latest,
      base: start.values,
      patch,
      overlap: again.overlap,
      changed: again.changedByOthers,
    };
  }

  function annotate(
    changed: readonly SettingsKey[],
    before: SettingsValues,
    patch: SettingsPatch,
  ): Elsewhere {
    const note: Elsewhere = {};
    for (const key of changed) {
      if (areaOfKey(key) === area && !patchKeys(patch).includes(key))
        note[key] = describeValue(key, before);
    }
    return note;
  }

  function handleOutcome(outcome: Outcome, patch: SettingsPatch): void {
    if (outcome.kind === "conflict") {
      const rows = outcome.overlap.map((key) => ({
        key,
        mine: describePatched(key, patch, outcome.fresh.values),
        theirs: describeValue(key, outcome.fresh.values),
        choice: "mine" as const,
      }));
      setElsewhere(annotate(outcome.changed, outcome.base, patch));
      setNotice({ kind: "conflict", rows });
      return;
    }
    const snapshot = outcome.kind === "saved" ? outcome.written : outcome.fresh;
    queryClient.setQueryData(queryKeys.settings.snapshot(), snapshot);
    void queryClient.invalidateQueries({ queryKey: queryKeys.settings.snapshot() });
    setDraft(EMPTY_DRAFT);
    setElsewhere(annotate(outcome.changed, outcome.base, patch));
    const reapplied = outcome.kind === "agreed" || outcome.reapplied;
    const restartKeys =
      outcome.kind === "saved" && outcome.restart
        ? patchKeys(patch).filter((key) => snapshot.restartRequiredFields.includes(key))
        : [];
    setNotice({
      kind: "saved",
      at: new Date(),
      reapplied,
      elsewhere: reapplied ? elsewhereSummary(area, outcome.changed, snapshot.values) : null,
      restartKeys,
    });
  }

  function handleError(err: unknown): void {
    if (err instanceof ApiError) {
      if (err.status === 401) {
        invokeUnauthenticatedHandler();
        setNotice({ kind: "none" });
        return;
      }
      if (err.status === 403) {
        setNotice({ kind: "forbidden" });
        return;
      }
      if (err.status === 422) {
        const key = fieldForServerDetail(area, err.detail);
        setNotice(
          key === null
            ? {
                kind: "validation",
                errors: {},
                unmapped: `The server did not accept this change: ${err.detail}.`,
              }
            : { kind: "validation", errors: { [key]: err.detail }, unmapped: null },
        );
        return;
      }
      setNotice({ kind: "failed", reason: "server" });
      return;
    }
    setNotice({ kind: "failed", reason: err instanceof TypeError ? "network" : "server" });
  }

  const mutation = useMutation({
    mutationFn: runSave,
    onSuccess: (outcome, vars) => {
      handleOutcome(outcome, vars.patch);
    },
    onError: handleError,
  });

  const pending = mutation.isPending;
  const phase = !pending ? "idle" : notice.kind === "reapplying" ? "reapplying" : "saving";

  function afterEdit(key: SettingsKey): void {
    setElsewhere({});
    setNotice((current) => {
      if (current.kind === "validation") {
        const errors = omitKey(current.errors, ALL_KEYS, key);
        const remaining = Object.keys(errors).length > 0;
        return remaining || current.unmapped !== null ? { ...current, errors } : { kind: "none" };
      }
      if (current.kind === "saved" || current.kind === "failed") return { kind: "none" };
      return current;
    });
  }

  function editBoolean(key: BooleanKey, value: boolean): void {
    if (pending) return;
    const shown = draft.booleans[key] ?? base.values[key];
    if (deletionCopy(key) !== null && value && !shown) {
      setPendingConfirm(key);
      return;
    }
    setDraft(setBoolean(draft, base.values, key, value));
    afterEdit(key);
  }

  function confirmDeletion(): void {
    if (pendingConfirm === null) return;
    setDraft(setBoolean(draft, base.values, pendingConfirm, true));
    afterEdit(pendingConfirm);
    setPendingConfirm(null);
  }

  function cancelDeletion(): void {
    setPendingConfirm(null);
  }

  function editText(key: NumberKey, text: string): void {
    if (pending) return;
    setDraft(setText(draft, base.values, key, text));
    afterEdit(key);
  }

  function editEpub(value: boolean): void {
    if (pending) return;
    setDraft(setEpub(draft, base.values, value));
    afterEdit("accepted_formats");
  }

  function discard(): void {
    if (pending) return;
    setDraft(EMPTY_DRAFT);
    setElsewhere({});
    setNotice({ kind: "none" });
  }

  function startSave(patch: SettingsPatch): void {
    setNotice({ kind: "none" });
    mutation.mutate({ base, patch });
  }

  function save(): void {
    if (pending) return;
    const built = buildPatch(area, draft, base.values);
    if (!built.ok) {
      setNotice({ kind: "validation", errors: built.errors, unmapped: null });
      return;
    }
    if (patchKeys(built.patch).length === 0) return;
    startSave(built.patch);
  }

  function choose(key: SettingsKey, choice: ConflictChoice): void {
    setNotice((current) =>
      current.kind === "conflict"
        ? {
            kind: "conflict",
            rows: current.rows.map((row) => (row.key === key ? { ...row, choice } : row)),
          }
        : current,
    );
  }

  function saveChoices(): void {
    if (pending || notice.kind !== "conflict") return;
    const theirs = notice.rows.filter((row) => row.choice === "theirs").map((row) => row.key);
    const next = dropKeys(draft, theirs);
    const built = buildPatch(area, next, base.values);
    if (!built.ok) {
      setDraft(next);
      setNotice({ kind: "validation", errors: built.errors, unmapped: null });
      return;
    }
    setDraft(next);
    setElsewhere({});
    if (patchKeys(built.patch).length === 0) {
      setNotice({ kind: "none" });
      return;
    }
    startSave(built.patch);
  }

  function discardAndReload(): void {
    if (pending) return;
    setDraft(EMPTY_DRAFT);
    setElsewhere({});
    setNotice({ kind: "none" });
    void queryClient.invalidateQueries({ queryKey: queryKeys.settings.snapshot() });
  }

  return {
    draft,
    dirty,
    notice,
    elsewhere,
    phase,
    pendingConfirm,
    editBoolean,
    editText,
    editEpub,
    confirmDeletion,
    cancelDeletion,
    discard,
    save,
    choose,
    saveChoices,
    discardAndReload,
  };
}
