import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import {
  DISPLAY_STORAGE_KEY,
  readDisplayPreferences,
  writeDisplayPreferences,
} from "./display-storage";

const NO_HINT = { density: null, hiddenColumns: null, view: null, sortStack: null };

afterEach(() => {
  localStorage.clear();
});

describe("display-storage", () => {
  test("round-trips every mirrored group", () => {
    writeDisplayPreferences({
      density: "compact",
      hiddenColumns: ["isbn_13", "pages"],
      view: "table",
      sortStack: "title,-pages",
    });
    expect(readDisplayPreferences()).toEqual({
      density: "compact",
      hiddenColumns: ["isbn_13", "pages"],
      view: "table",
      sortStack: "title,-pages",
    });
  });

  test("absent storage reads as no hint", () => {
    expect(readDisplayPreferences()).toEqual(NO_HINT);
  });

  test("a density write preserves the other mirrored groups", () => {
    writeDisplayPreferences({ hiddenColumns: ["rating"], view: "table" });
    writeDisplayPreferences({ density: "comfortable" });
    expect(readDisplayPreferences()).toEqual({
      density: "comfortable",
      hiddenColumns: ["rating"],
      view: "table",
      sortStack: null,
    });
  });

  test("malformed JSON reads as no hint", () => {
    localStorage.setItem(DISPLAY_STORAGE_KEY, "{not json");
    expect(readDisplayPreferences()).toEqual(NO_HINT);
  });

  test("unknown density value degrades to null without dropping columns", () => {
    localStorage.setItem(
      DISPLAY_STORAGE_KEY,
      JSON.stringify({ density: "roomy", hiddenColumns: ["series"] }),
    );
    expect(readDisplayPreferences()).toEqual({ ...NO_HINT, hiddenColumns: ["series"] });
  });

  test("unknown view value degrades to null without dropping density", () => {
    localStorage.setItem(DISPLAY_STORAGE_KEY, JSON.stringify({ density: "compact", view: "list" }));
    expect(readDisplayPreferences()).toEqual({ ...NO_HINT, density: "compact" });
  });

  test("non-string entries in hiddenColumns degrade the field to null", () => {
    localStorage.setItem(
      DISPLAY_STORAGE_KEY,
      JSON.stringify({ density: "compact", hiddenColumns: [3, "series"] }),
    );
    expect(readDisplayPreferences()).toEqual({ ...NO_HINT, density: "compact" });
  });

  test("a payload written before the mirror covered view and sort still reads", () => {
    localStorage.setItem(
      DISPLAY_STORAGE_KEY,
      JSON.stringify({ density: "compact", hiddenColumns: ["series"] }),
    );
    expect(readDisplayPreferences()).toEqual({
      density: "compact",
      hiddenColumns: ["series"],
      view: null,
      sortStack: null,
    });
  });

  test("throwing storage degrades silently on read and write", () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(readDisplayPreferences()).toEqual(NO_HINT);
    expect(() => {
      writeDisplayPreferences({ density: "compact" });
    }).not.toThrow();
    spy.mockRestore();
  });
});
