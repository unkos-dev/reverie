import { beforeEach, describe, expect, test, vi } from "vite-plus/test";

import { __seedCsrfTokenForTesting } from "@/api/csrf";

function jsonResponse(body: unknown, init?: ResponseInit): Response {
  return new Response(JSON.stringify(body), {
    status: 200,
    headers: { "Content-Type": "application/json" },
    ...init,
  });
}

/** Parse the JSON request body recorded for a mocked fetch call. */
function bodyJson(init: RequestInit | undefined): unknown {
  const body = init?.body;
  if (typeof body !== "string") throw new Error("expected a serialized JSON body");
  return JSON.parse(body);
}

const STUB_USER = {
  id: "aaaaaaaa-aaaa-4aaa-8aaa-aaaaaaaaaaaa",
  display_name: "Alice",
  email: "alice@example.com",
  role: "admin",
  is_child: false,
  disabled: false,
  has_password: true,
  credential_link_expires_at: null,
  created_at: "2026-05-25T00:00:00Z",
  updated_at: "2026-05-25T00:00:00Z",
};

beforeEach(() => {
  // Seed (not reset): apiFetch lazily hydrates an empty cache with a
  // leading /auth/me fetch that would eat these suites' response mocks;
  // hydration behaviour itself is pinned in fetch.test.ts.
  __seedCsrfTokenForTesting("test-csrf-token-0000000000000000000000000");
  vi.restoreAllMocks();
});

describe("credential link email", () => {
  const jobId = "bbbbbbbb-bbbb-4bbb-8bbb-bbbbbbbbbbbb";
  test("posts only the held token and reads only the account's job", async () => {
    const { sendCredentialLinkEmail, fetchCredentialLinkEmail } = await import("@/api/users");
    const queued = { job_id: jobId, status: "queued", failure_category: null };
    const request = vi
      .spyOn(globalThis, "fetch")
      .mockResolvedValueOnce(jsonResponse(queued, { status: 202 }))
      .mockResolvedValueOnce(jsonResponse({ ...queued, status: "accepted" }));
    expect(await sendCredentialLinkEmail(STUB_USER.id, "A".repeat(43))).toEqual(queued);
    expect(bodyJson(request.mock.calls[0][1])).toEqual({ token: "A".repeat(43) });
    expect(request.mock.calls[0][0]).toBe(`/api/v1/users/${STUB_USER.id}/credential-links/emails`);
    expect((await fetchCredentialLinkEmail(STUB_USER.id, jobId)).status).toBe("accepted");
    expect(request.mock.calls[1][0]).toBe(
      `/api/v1/users/${STUB_USER.id}/credential-links/emails/${jobId}`,
    );
  });
});
