import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen } from "vite-plus/test/browser";
import { beforeEach, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter } from "react-router";
import { ApiError } from "@/api";
import { confirmEmailVerification, fetchEmailVerification } from "@/api/auth";
import { Component } from "@/routes/verify-email";

vi.mock("@/api/auth");
vi.mock("@/lib/theme/ThemeProvider", () => ({ useTheme: () => ({ effective: "dark" }) }));
const token = "A".repeat(43);
async function page(value = token): Promise<QueryClient> {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createMemoryRouter(
    [{ path: "/verify-email", loader: () => value, element: <Component /> }],
    { initialEntries: ["/verify-email"] },
  );
  await render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return client;
}
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(fetchEmailVerification).mockResolvedValue({
    email: "holder@example.test",
    email_verified: false,
    mail_enabled: true,
  });
  vi.mocked(confirmEmailVerification).mockResolvedValue(undefined);
});
test("asks a signed-out person to reopen the original email after sign-in", async () => {
  vi.mocked(fetchEmailVerification).mockRejectedValue(new ApiError(401, null, "Sign in", ""));
  await page();
  await expect
    .element(screen.getByRole("heading", { name: "Sign in to verify your email" }))
    .toBeInTheDocument();
  await expect
    .element(screen.getByRole("link", { name: "Go to sign in" }))
    .toHaveAttribute("href", "/login");
  expect(Object.values(window.localStorage).join(" ")).not.toContain(token);
  expect(Object.values(window.sessionStorage).join(" ")).not.toContain(token);
  expect(confirmEmailVerification).not.toHaveBeenCalled();
});
