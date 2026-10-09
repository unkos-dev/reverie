import { describe, expect, test } from "vite-plus/test";

import type { SettingsValues } from "@/api/settings";

import {
  FIELDS,
  areaSummary,
  describeValue,
  fieldByKey,
  fieldCountText,
  fieldsForArea,
  AreaIdSchema,
  numberToText,
  omitKey,
  parseInteger,
  parseMib,
  restartLabels,
  type IntField,
} from "./fields";

const VALUES: SettingsValues = {
  accepted_formats: ["epub"],
  cleanup_imported: false,
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

function intField(key: IntField["key"]): IntField {
  const field = fieldByKey(key);
  if (field.control !== "int") throw new Error(`${key} is not an integer field`);
  return field;
}

describe("catalogue", () => {
  test("every API field belongs to exactly one area and is catalogued once", () => {
    const keys = FIELDS.map((field) => field.key);
    expect(new Set(keys).size).toBe(keys.length);
    expect(keys).toHaveLength(Object.keys(VALUES).length);
    for (const key of Object.keys(VALUES)) expect(keys).toContain(key);
  });

  test("the provider area is not routable until its registry endpoint exists", () => {
    expect(AreaIdSchema.safeParse("providers").success).toBe(false);
    expect(AreaIdSchema.safeParse("covers").success).toBe(true);
    expect(AreaIdSchema.safeParse(undefined).success).toBe(false);
  });

  test("areas hold the fields the spec assigns them", () => {
    expect(fieldsForArea("acquisition").map((f) => f.key)).toEqual([
      "accepted_formats",
      "cleanup_imported",
      "cleanup_duplicates",
    ]);
    expect(fieldsForArea("catalogue").map((f) => f.key)).toEqual([
      "opds_enabled",
      "opds_page_size",
    ]);
  });
});

describe("parseInteger", () => {
  const concurrency = intField("enrichment_concurrency");
  const redirects = intField("cover_redirect_limit");
  const poll = intField("enrichment_poll_idle_secs");

  test("accepts values inside the bounds, including both edges", () => {
    expect(parseInteger(concurrency, "1")).toEqual({ ok: true, value: 1 });
    expect(parseInteger(concurrency, " 10 ")).toEqual({ ok: true, value: 10 });
    expect(parseInteger(redirects, "0")).toEqual({ ok: true, value: 0 });
  });

  test("rejects out-of-range values with the bounded message", () => {
    expect(parseInteger(concurrency, "0")).toEqual({
      ok: false,
      message: "Enter a whole number from 1 to 10.",
    });
    expect(parseInteger(concurrency, "11")).toEqual({
      ok: false,
      message: "Enter a whole number from 1 to 10.",
    });
    expect(parseInteger(poll, "0")).toEqual({
      ok: false,
      message: "Enter a whole number of 1 or more.",
    });
    expect(parseInteger(redirects, "-1")).toEqual({
      ok: false,
      message: "Enter a whole number of 0 or more.",
    });
  });

  test("rejects fractions and exponents as not whole, and non-numbers as not a number", () => {
    expect(parseInteger(concurrency, "2.5")).toEqual({
      ok: false,
      message: "Enter a whole number from 1 to 10.",
    });
    expect(parseInteger(poll, "1e2")).toEqual({
      ok: false,
      message: "Enter a whole number of 1 or more.",
    });
    expect(parseInteger(poll, "abc")).toEqual({ ok: false, message: "Enter a number." });
    expect(parseInteger(poll, "")).toEqual({ ok: false, message: "Enter a number." });
    expect(parseInteger(poll, "   ")).toEqual({ ok: false, message: "Enter a number." });
  });

  test("rejects values beyond the 32-bit range the server stores", () => {
    expect(parseInteger(poll, "2147483647")).toEqual({ ok: true, value: 2147483647 });
    expect(parseInteger(poll, "2147483648")).toEqual({
      ok: false,
      message: "Enter a whole number no larger than 2,147,483,647.",
    });
  });
});

describe("parseMib", () => {
  test("converts MiB to exact bytes", () => {
    expect(parseMib("10")).toEqual({ ok: true, value: 10_485_760 });
    expect(parseMib("0.5")).toEqual({ ok: true, value: 524_288 });
    expect(parseMib(".5")).toEqual({ ok: true, value: 524_288 });
  });

  test("an exact byte count survives a display and parse round trip", () => {
    const bytes = 1_500_000;
    expect(parseMib(numberToText("cover_max_bytes", bytes))).toEqual({ ok: true, value: bytes });
  });

  test("rejects zero, negatives and values that round to zero bytes", () => {
    for (const text of ["0", "-1", "0.0000001"]) {
      expect(parseMib(text)).toEqual({ ok: false, message: "Enter a size greater than 0." });
    }
  });

  test("rejects non-numbers and sizes beyond the exact integer range", () => {
    expect(parseMib("big")).toEqual({ ok: false, message: "Enter a number." });
    expect(parseMib("")).toEqual({ ok: false, message: "Enter a number." });
    const tooBig = parseMib("9000000000");
    expect(tooBig.ok).toBe(false);
    if (!tooBig.ok) expect(tooBig.message).toBe("Enter a size no larger than 8,589,934,591 MiB.");
  });
});

describe("areaSummary", () => {
  test("acquisition reflects the two source-file switches", () => {
    expect(areaSummary("acquisition", VALUES)).toBe("EPUB. Source files kept");
    expect(areaSummary("acquisition", { ...VALUES, cleanup_imported: true })).toBe(
      "EPUB. Source files removed after import",
    );
    expect(areaSummary("acquisition", { ...VALUES, cleanup_duplicates: true })).toBe(
      "EPUB. Source files removed",
    );
    expect(
      areaSummary("acquisition", { ...VALUES, cleanup_imported: true, cleanup_duplicates: true }),
    ).toBe("EPUB. Source files removed");
    expect(areaSummary("acquisition", { ...VALUES, accepted_formats: [] })).toBe(
      "No formats. Source files kept",
    );
  });

  test("worker areas read On with a count, or Off", () => {
    expect(areaSummary("enrichment", VALUES)).toBe("On. 4 lookups at a time");
    expect(areaSummary("enrichment", { ...VALUES, enrichment_concurrency: 1 })).toBe(
      "On. 1 lookup at a time",
    );
    expect(areaSummary("enrichment", { ...VALUES, enrichment_enabled: false })).toBe("Off");
    expect(areaSummary("writeback", VALUES)).toBe("On. 2 files at a time");
    expect(areaSummary("catalogue", VALUES)).toBe("On. 50 entries per page");
    expect(areaSummary("catalogue", { ...VALUES, opds_enabled: false })).toBe("Off");
  });

  test("covers shows the size limit in MiB", () => {
    expect(areaSummary("covers", VALUES)).toBe("Up to 10 MiB");
    expect(areaSummary("covers", { ...VALUES, cover_max_bytes: 1_572_864 })).toBe("Up to 1.5 MiB");
  });
});

describe("copy helpers", () => {
  test("describeValue quotes each control kind", () => {
    expect(describeValue("enrichment_enabled", VALUES)).toBe("On");
    expect(describeValue("enrichment_concurrency", VALUES)).toBe("4 lookups");
    expect(describeValue("cover_max_bytes", VALUES)).toBe("10 MiB");
    expect(describeValue("accepted_formats", VALUES)).toBe("EPUB");
    expect(describeValue("accepted_formats", { ...VALUES, accepted_formats: [] })).toBe("None");
  });

  test("fieldCountText and restartLabels pluralise and join", () => {
    expect(fieldCountText(1)).toBe("1 field needs attention");
    expect(fieldCountText(2)).toBe("2 fields need attention");
    expect(restartLabels(["cover_max_bytes", "opds_page_size"])).toBe(
      "Largest cover file and Entries per page",
    );
  });

  test("omitKey drops one key without touching the input", () => {
    const record = { a: 1, b: 2 };
    expect(omitKey(record, ["a", "b"], "a")).toEqual({ b: 2 });
    expect(record).toEqual({ a: 1, b: 2 });
  });
});
