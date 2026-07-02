import { describe, expect, test, vi } from "vite-plus/test";

import { listTokens, createToken } from "./tokens";

vi.mock("./fetch", () => ({ apiFetch: vi.fn() }));
// eslint-disable-next-line import/first -- imported after the mock is registered
import { apiFetch } from "./fetch";

const VALID_TOKEN = {
  id: "cccccccc-cccc-4ccc-8ccc-cccccccccccc",
  name: "My Kindle",
  scopes: ["read"],
  expires_at: null,
  last_used_at: null,
  created_at: "2026-05-25T00:00:00Z",
};

describe("tokens API client parsing", () => {
  test("listTokens parses a well-formed payload", async () => {
    vi.mocked(apiFetch).mockResolvedValue([VALID_TOKEN]);
    await expect(listTokens()).resolves.toEqual([VALID_TOKEN]);
  });

  test("listTokens rejects an unknown scope", async () => {
    vi.mocked(apiFetch).mockResolvedValue([{ ...VALID_TOKEN, scopes: ["superuser"] }]);
    await expect(listTokens()).rejects.toThrow();
  });

  test("listTokens rejects a malformed timestamp", async () => {
    vi.mocked(apiFetch).mockResolvedValue([{ ...VALID_TOKEN, created_at: "not-a-date" }]);
    await expect(listTokens()).rejects.toThrow();
  });

  test("createToken parses the one-time credential in the mint response", async () => {
    vi.mocked(apiFetch).mockResolvedValue({
      ...VALID_TOKEN,
      token: "rvpat_11111111-1111-4111-8111-111111111111.secretvalue",
    });
    const result = await createToken({ name: "k", scopes: ["read"], expiresInDays: null });
    expect(typeof result.token).toBe("string");
  });

  test("createToken rejects a mint response missing the credential", async () => {
    vi.mocked(apiFetch).mockResolvedValue(VALID_TOKEN);
    await expect(
      createToken({ name: "k", scopes: ["read"], expiresInDays: null }),
    ).rejects.toThrow();
  });
});
