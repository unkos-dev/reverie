import { describe, expect, test } from "vite-plus/test";

import { currentPasswordField, newPasswordField } from "./auth.schemas";

describe("configured new-password length", () => {
  const policy = { password_min_length: 15, password_max_length: 64 };

  test("accepts the bounds and counts Unicode scalar values", () => {
    const schema = newPasswordField(policy);
    expect(schema.safeParse("a".repeat(15)).success).toBe(true);
    expect(schema.safeParse("a".repeat(64)).success).toBe(true);
    expect(schema.safeParse("😀".repeat(15)).success).toBe(true);
    expect(schema.safeParse("😀".repeat(14)).success).toBe(false);
    expect(schema.safeParse("a".repeat(65)).success).toBe(false);
  });

  test("follows a stricter configured minimum without changing current-password rules", () => {
    expect(
      newPasswordField({ ...policy, password_min_length: 24 }).safeParse("a".repeat(23)).success,
    ).toBe(false);
    expect(
      newPasswordField({ ...policy, password_min_length: 24 }).safeParse("a".repeat(24)).success,
    ).toBe(true);
    expect(currentPasswordField.safeParse("old").success).toBe(true);
  });
});
