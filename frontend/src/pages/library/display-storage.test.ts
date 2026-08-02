import { afterEach, describe, expect, test, vi } from "vite-plus/test";

import {
  DISPLAY_STORAGE_KEY,
  readDisplayPreferences,
  writeDisplayPreferences,
} from "./display-storage";

afterEach(() => {
  localStorage.clear();
});

describe("display-storage", () => {
  test("round-trips density and hidden columns", () => {
    writeDisplayPreferences({ density: "compact", hiddenColumns: ["isbn_13", "pages"] });
    expect(readDisplayPreferences()).toEqual({
      density: "compact",
      hiddenColumns: ["isbn_13", "pages"],
    });
  });

  test("absent storage reads as no preference", () => {
    expect(readDisplayPreferences()).toEqual({ density: null, hiddenColumns: null });
  });

  test("a density write preserves stored hidden columns", () => {
    writeDisplayPreferences({ hiddenColumns: ["rating"] });
    writeDisplayPreferences({ density: "comfortable" });
    expect(readDisplayPreferences()).toEqual({
      density: "comfortable",
      hiddenColumns: ["rating"],
    });
  });

  test("malformed JSON reads as no preference", () => {
    localStorage.setItem(DISPLAY_STORAGE_KEY, "{not json");
    expect(readDisplayPreferences()).toEqual({ density: null, hiddenColumns: null });
  });

  test("unknown density value degrades to null without dropping columns", () => {
    localStorage.setItem(
      DISPLAY_STORAGE_KEY,
      JSON.stringify({ density: "roomy", hiddenColumns: ["series"] }),
    );
    expect(readDisplayPreferences()).toEqual({ density: null, hiddenColumns: ["series"] });
  });

  test("non-string entries in hiddenColumns degrade the field to null", () => {
    localStorage.setItem(
      DISPLAY_STORAGE_KEY,
      JSON.stringify({ density: "compact", hiddenColumns: [3, "series"] }),
    );
    expect(readDisplayPreferences()).toEqual({ density: "compact", hiddenColumns: null });
  });

  test("throwing storage degrades silently on read and write", () => {
    const spy = vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new Error("denied");
    });
    expect(readDisplayPreferences()).toEqual({ density: null, hiddenColumns: null });
    expect(() => {
      writeDisplayPreferences({ density: "compact" });
    }).not.toThrow();
    spy.mockRestore();
  });
});
