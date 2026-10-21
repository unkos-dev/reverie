import { describe, expect, test } from "vite-plus/test";

import { currentPasswordField, emailField, newPasswordField } from "./auth.schemas";

describe("account email format", () => {
  test.each([
    "john@example.com",
    "john.doe+books@example.com",
    "o'brien@example.com",
    "Jane_Doe@EXAMPLE.COM",
  ])("accepts %s", (email) => {
    expect(emailField.safeParse(email).success).toBe(true);
  });

  test.each([
    '"john doe"@example.com',
    '"john"@example.com',
    "john doe@example.com",
    "jane@homelab",
    "jane!books@example.com",
    "jane#books@example.com",
    "jane%books@example.com",
    "jane/books@example.com",
    "josé@example.com",
  ])("rejects %s", (email) => {
    expect(emailField.safeParse(email).success).toBe(false);
  });
});

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
