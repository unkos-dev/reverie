import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page as screen } from "vite-plus/test/browser";
import { beforeEach, afterEach, expect, test, vi } from "vite-plus/test";
import { createMemoryRouter, RouterProvider } from "react-router";

import { fetchSetupStatus, lookupCredentialLink } from "@/api/auth";

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
