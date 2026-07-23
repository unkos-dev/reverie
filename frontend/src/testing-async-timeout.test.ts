/**
 * Pins the suite-wide async ceiling set in `tests/setup.ts`. The value is
 * invisible at every call site, so nothing else would notice if the setup
 * import were dropped or the option renamed, and the suite would silently
 * return to the 1s default that fails under load.
 */
import { getConfig, screen } from "@testing-library/react";
import { expect, test } from "vite-plus/test";

test("async queries use the suite-wide ceiling, not the 1s default", () => {
  expect(getConfig().asyncUtilTimeout).toBe(5000);
});

test("a query that never resolves waits for the configured ceiling", async () => {
  const started = performance.now();
  await expect(screen.findByRole("heading", { name: "absent" })).rejects.toThrow();
  const waited = performance.now() - started;

  // Bounded on both sides: below the 1s default proves the ceiling is not
  // merely declared, and the upper bound catches a runaway value.
  expect(waited).toBeGreaterThan(1500);
  expect(waited).toBeLessThan(8000);
});
