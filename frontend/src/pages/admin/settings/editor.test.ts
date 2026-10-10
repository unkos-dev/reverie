import { describe, expect, test } from "vite-plus/test";

import type { SettingsValues } from "@/api/settings";

import {
  EMPTY_DRAFT,
  buildPatch,
  changedKeys,
  classify,
  describeDraftValue,
  dirtyKeys,
  dropKeys,
  elsewhereSummary,
  fieldForServerDetail,
  patchKeys,
  setBoolean,
  setEpub,
  setText,
  shownBoolean,
  shownEpub,
  shownText,
  withoutKeys,
} from "./editor";

const BASE: SettingsValues = {
  accepted_formats: ["epub"],
  cleanup_imported: true,
  cleanup_duplicates: false,
  enrichment_enabled: true,
  enrichment_concurrency: 4,
  enrichment_poll_idle_secs: 30,
  enrichment_fetch_budget_secs: 20,
  cover_max_bytes: 10 * 1_048_576,
  cover_download_timeout_secs: 15,
  cover_min_long_edge_px: 600,
  cover_redirect_limit: 3,
  writeback_enabled: true,
  writeback_concurrency: 2,
  writeback_poll_idle_secs: 30,
  writeback_max_attempts: 5,
  opds_enabled: true,
  opds_page_size: 50,
};

describe("draft edits", () => {
  test("an edit back to the saved value leaves nothing dirty", () => {
    let draft = setText(EMPTY_DRAFT, BASE, "enrichment_concurrency", "6");
    expect(dirtyKeys("enrichment", draft, BASE)).toEqual(["enrichment_concurrency"]);
    draft = setText(draft, BASE, "enrichment_concurrency", "4");
    expect(dirtyKeys("enrichment", draft, BASE)).toEqual([]);
    expect(draft.numbers).toEqual({});
  });

  test("a different spelling of the saved number is not a change", () => {
    const draft = setText(EMPTY_DRAFT, BASE, "enrichment_concurrency", "04");
    expect(dirtyKeys("enrichment", draft, BASE)).toEqual([]);
    expect(shownText(draft, BASE, "enrichment_concurrency")).toBe("04");
  });

  test("unparsable text counts as dirty so Save can report it", () => {
    const draft = setText(EMPTY_DRAFT, BASE, "enrichment_concurrency", "abc");
    expect(dirtyKeys("enrichment", draft, BASE)).toEqual(["enrichment_concurrency"]);
  });

  test("booleans and the EPUB checkbox track dirtiness against the base", () => {
    let draft = setBoolean(EMPTY_DRAFT, BASE, "cleanup_duplicates", true);
    draft = setEpub(draft, BASE, false);
    expect(dirtyKeys("acquisition", draft, BASE)).toEqual([
      "accepted_formats",
      "cleanup_duplicates",
    ]);
    expect(shownBoolean(draft, BASE, "cleanup_duplicates")).toBe(true);
    expect(shownEpub(draft, BASE)).toBe(false);
    draft = setBoolean(draft, BASE, "cleanup_duplicates", false);
    draft = setEpub(draft, BASE, true);
    expect(dirtyKeys("acquisition", draft, BASE)).toEqual([]);
  });

  test("dirtiness is scoped to the open area", () => {
    const draft = setText(EMPTY_DRAFT, BASE, "opds_page_size", "25");
    expect(dirtyKeys("enrichment", draft, BASE)).toEqual([]);
    expect(dirtyKeys("catalogue", draft, BASE)).toEqual(["opds_page_size"]);
  });

  test("dropKeys removes only the named fields", () => {
    let draft = setText(EMPTY_DRAFT, BASE, "enrichment_concurrency", "6");
    draft = setText(draft, BASE, "enrichment_poll_idle_secs", "45");
    const next = dropKeys(draft, ["enrichment_concurrency"]);
    expect(dirtyKeys("enrichment", next, BASE)).toEqual(["enrichment_poll_idle_secs"]);
    expect(dirtyKeys("enrichment", draft, BASE)).toHaveLength(2);
  });
});

describe("buildPatch", () => {
  test("sends only dirty fields, parsed to API values", () => {
    let draft = setText(EMPTY_DRAFT, BASE, "cover_max_bytes", "12.5");
    draft = setText(draft, BASE, "cover_redirect_limit", "0");
    expect(buildPatch("covers", draft, BASE)).toEqual({
      ok: true,
      patch: { cover_max_bytes: 13_107_200, cover_redirect_limit: 0 },
    });
  });

  test("an empty draft builds an empty patch, never a request body", () => {
    expect(buildPatch("covers", EMPTY_DRAFT, BASE)).toEqual({ ok: true, patch: {} });
  });

  test("reports every invalid field together", () => {
    let draft = setText(EMPTY_DRAFT, BASE, "enrichment_concurrency", "12");
    draft = setText(draft, BASE, "enrichment_poll_idle_secs", "0");
    draft = setText(draft, BASE, "enrichment_fetch_budget_secs", "21");
    expect(buildPatch("enrichment", draft, BASE)).toEqual({
      ok: false,
      errors: {
        enrichment_concurrency: "Enter a whole number from 1 to 10.",
        enrichment_poll_idle_secs: "Enter a whole number of 1 or more.",
      },
    });
  });

  test("an unticked EPUB sends an empty format list", () => {
    const draft = setEpub(EMPTY_DRAFT, BASE, false);
    expect(buildPatch("acquisition", draft, BASE)).toEqual({
      ok: true,
      patch: { accepted_formats: [] },
    });
  });
});

describe("classify", () => {
  const draft = setText(
    setText(EMPTY_DRAFT, BASE, "enrichment_concurrency", "6"),
    BASE,
    "enrichment_fetch_budget_secs",
    "30",
  );
  const built = buildPatch("enrichment", draft, BASE);
  const patch = built.ok ? built.patch : {};

  test("a field changed only by the other admin is not an overlap", () => {
    const fresh = { ...BASE, enrichment_poll_idle_secs: 45, writeback_max_attempts: 3 };
    const found = classify(BASE, fresh, patch);
    expect(found.overlap).toEqual([]);
    expect(found.agreed).toEqual([]);
    expect(found.changedByOthers).toEqual(["enrichment_poll_idle_secs", "writeback_max_attempts"]);
  });

  test("a dirty field the other admin set to a different value is an overlap", () => {
    const found = classify(BASE, { ...BASE, enrichment_concurrency: 8 }, patch);
    expect(found.overlap).toEqual(["enrichment_concurrency"]);
  });

  test("a dirty field the other admin set to the user's own value is agreed, not a conflict", () => {
    const found = classify(BASE, { ...BASE, enrichment_concurrency: 6 }, patch);
    expect(found.overlap).toEqual([]);
    expect(found.agreed).toEqual(["enrichment_concurrency"]);
  });

  test("an unchanged server reports nothing", () => {
    expect(classify(BASE, BASE, patch)).toEqual({ overlap: [], agreed: [], changedByOthers: [] });
  });

  test("withoutKeys and patchKeys round-trip a patch", () => {
    expect(patchKeys(patch)).toEqual(["enrichment_concurrency", "enrichment_fetch_budget_secs"]);
    expect(withoutKeys(patch, ["enrichment_concurrency"])).toEqual({
      enrichment_fetch_budget_secs: 30,
    });
  });

  test("changedKeys compares format lists by content", () => {
    expect(changedKeys(BASE, { ...BASE, accepted_formats: ["epub"] })).toEqual([]);
    expect(changedKeys(BASE, { ...BASE, accepted_formats: [] })).toEqual(["accepted_formats"]);
  });
});

describe("describeDraftValue", () => {
  test("quotes each control kind as the draft holds it", () => {
    const draft = setEpub(
      setBoolean(
        setText(EMPTY_DRAFT, BASE, "enrichment_concurrency", "9"),
        BASE,
        "cleanup_imported",
        false,
      ),
      BASE,
      false,
    );
    expect(describeDraftValue("enrichment_concurrency", draft, BASE)).toBe("9 lookups");
    expect(describeDraftValue("cleanup_imported", draft, BASE)).toBe("Off");
    expect(describeDraftValue("accepted_formats", draft, BASE)).toBe("None");
    expect(describeDraftValue("writeback_enabled", draft, BASE)).toBe("On");
  });

  test("quotes unparsable text as typed", () => {
    const draft = setText(EMPTY_DRAFT, BASE, "enrichment_concurrency", "nine");
    expect(describeDraftValue("enrichment_concurrency", draft, BASE)).toBe("nine");
  });
});

describe("elsewhereSummary", () => {
  test("names other-area fields with their new values, capped at three", () => {
    const fresh = {
      ...BASE,
      writeback_max_attempts: 3,
      cover_redirect_limit: 5,
      opds_page_size: 25,
      cover_download_timeout_secs: 20,
    };
    const text = elsewhereSummary("enrichment", changedKeys(BASE, fresh), fresh);
    expect(text).toBe(
      "Also changed by someone else: Covers, Download timeout, now 20 seconds; Covers, Redirects to follow, now 5 redirects; Writeback, Attempts per file, now 3 attempts, and 1 more.",
    );
  });

  test("ignores fields on the open page and returns null when nothing is elsewhere", () => {
    const fresh = { ...BASE, enrichment_poll_idle_secs: 45 };
    expect(elsewhereSummary("enrichment", changedKeys(BASE, fresh), fresh)).toBeNull();
  });
});

describe("fieldForServerDetail", () => {
  test("maps a server message to the field it names within the open area", () => {
    expect(
      fieldForServerDetail("enrichment", "enrichment_concurrency must be between 1 and 10"),
    ).toBe("enrichment_concurrency");
  });

  test("returns null for another area's field or an unattributed message", () => {
    expect(
      fieldForServerDetail("covers", "enrichment_concurrency must be between 1 and 10"),
    ).toBeNull();
    expect(fieldForServerDetail("enrichment", "request body is not valid")).toBeNull();
  });
});
