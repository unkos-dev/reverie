import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { beforeEach, afterEach, expect, test, vi } from "vite-plus/test";
import { createMemoryRouter, RouterProvider } from "react-router";

import { ApiError } from "@/api";
import { fetchSetupStatus, lookupCredentialLink, redeemCredentialLink } from "@/api/auth";

import { Component } from "@/routes/set-password";
import { credentialLinkLoader as loader } from "@/route-table";

vi.mock("@/api/auth");
vi.mock("@/lib/theme/ThemeProvider", () => ({
  useTheme: () => ({ effective: "dark", preference: "system", setPreference: vi.fn() }),
}));

beforeEach(() => {
  vi.resetAllMocks();
  window.history.replaceState(null, "", "/set-password#test-link");
  vi.mocked(fetchSetupStatus).mockResolvedValue({
    setup_required: false,
    local_auth_enabled: true,
    oidc_enabled: false,
    email_recovery_enabled: false,
    password_min_length: 15,
    password_max_length: 256,
  });
});
afterEach(cleanup);

async function renderPage(path = "/set-password"): Promise<QueryClient> {
  window.history.replaceState(null, "", path + window.location.hash);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createMemoryRouter([{ path, Component, loader }], {
    initialEntries: [path],
  });
  await render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return client;
}

test("checks the link and clears the fragment", async () => {
  vi.mocked(lookupCredentialLink).mockReturnValue(new Promise(() => {}));
  await renderPage();
  await expect.element(screen.getByRole("status", { name: "Checking link" })).toBeInTheDocument();
  expect(window.location.hash).toBe("");
  await vi.waitFor(() => {
    expect(lookupCredentialLink).toHaveBeenCalledWith("test-link", expect.any(AbortSignal));
  });
});

test.each([
  new TypeError("Failed to fetch"),
  new ApiError(503, null, "Busy", "Try later"),
  new ApiError(429, null, "Limited", "Try later"),
])("retries a temporary lookup failure without restoring the fragment: %s", async (error) => {
  vi.mocked(lookupCredentialLink).mockRejectedValueOnce(error);
  await renderPage();
  const user = userEvent;
  const retry = screen.getByRole("button", { name: "Try again" });
  let complete: ((account: { email: string; has_password: boolean }) => void) | undefined;
  vi.mocked(lookupCredentialLink).mockReturnValueOnce(
    new Promise((resolve) => {
      complete = resolve;
    }),
  );
  await user.click(retry);
  await expect.element(retry).toBeDisabled();
  expect(window.location.hash).toBe("");
  expect(redeemCredentialLink).not.toHaveBeenCalled();

  complete?.({ email: "bob@example.com", has_password: false });
  await Promise.resolve();

  await expect.element(screen.getByLabelText("New password")).toBeInTheDocument();
  await vi.waitFor(() => {
    expect(lookupCredentialLink).toHaveBeenCalledTimes(2);
  });
  expect(lookupCredentialLink).toHaveBeenLastCalledWith("test-link", expect.any(AbortSignal));
});
