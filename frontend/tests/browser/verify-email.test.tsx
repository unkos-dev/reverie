import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { beforeEach, expect, test, vi } from "vite-plus/test";
import { RouterProvider, createMemoryRouter } from "react-router";
import { ApiError } from "@/api";
import { confirmEmailVerification, fetchEmailVerification } from "@/api/auth";
import { queryKeys } from "@/lib/query/keys";
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
test("confirms only on activation and keeps success through later query errors", async () => {
  const client = await page();
  const user = userEvent;
  await expect
    .element(screen.getByText("Confirm holder@example.test as the email address for your account."))
    .toBeInTheDocument();
  expect(confirmEmailVerification).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Verify email" }));
  await expect
    .element(screen.getByRole("heading", { name: "Email address verified" }))
    .toHaveFocus();
  expect(confirmEmailVerification).toHaveBeenCalledExactlyOnceWith(token);
  vi.mocked(fetchEmailVerification).mockRejectedValue(new Error("late failure"));

  await expect(
    client.query({
      queryKey: queryKeys.auth.emailVerification(),
      queryFn: () => fetchEmailVerification(),
      retry: false,
    }),
  ).rejects.toThrow("late failure");

  await expect
    .element(screen.getByRole("heading", { name: "Email address verified" }))
    .toBeInTheDocument();
  expect(confirmEmailVerification).toHaveBeenCalledTimes(1);
});
test.each(["", "invalid", "A".repeat(42) + "B"])(
  "refuses malformed tokens without a session request",
  async (value) => {
    await page(value);
    await expect
      .element(screen.getByRole("heading", { name: "This link can't be used" }))
      .toBeInTheDocument();
    expect(fetchEmailVerification).not.toHaveBeenCalled();
    expect(confirmEmailVerification).not.toHaveBeenCalled();
  },
);
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
test("uses one invalid state for a wrong-account, expired or used challenge", async () => {
  vi.mocked(confirmEmailVerification).mockRejectedValue(
    new ApiError(422, null, "Invalid", "invalid or expired link"),
  );
  await page();
  const user = userEvent;
  await user.click(screen.getByRole("button", { name: "Verify email" }));
  await expect
    .element(screen.getByRole("heading", { name: "This link can't be used" }))
    .toBeInTheDocument();
  await expect
    .element(screen.getByRole("link", { name: "Go to email settings" }))
    .toHaveAttribute("href", "/account/email");
});
test("does not infer success or retry after a lost response", async () => {
  vi.mocked(confirmEmailVerification).mockRejectedValue(new TypeError("lost acknowledgement"));
  await page();
  const user = userEvent;
  await user.click(screen.getByRole("button", { name: "Verify email" }));
  await expect
    .element(screen.getByRole("alert"))
    .toHaveTextContent(
      "Your email address may already be verified. Check your email settings before trying again.",
    );
  await expect.element(screen.getByRole("button", { name: "Verify email" })).toBeEnabled();
  expect(confirmEmailVerification).toHaveBeenCalledTimes(1);
});
test("renders a rate limit without discarding the challenge", async () => {
  vi.mocked(confirmEmailVerification).mockRejectedValue(
    new ApiError(429, null, "Wait", "Wait a minute."),
  );
  await page();
  const user = userEvent;
  await user.click(screen.getByRole("button", { name: "Verify email" }));
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Wait a minute.");
  await expect.element(screen.getByRole("button", { name: "Verify email" })).toBeEnabled();
});
test("disables duplicate confirmations while pending", async () => {
  vi.mocked(confirmEmailVerification).mockReturnValue(new Promise(() => {}));
  await page();
  const user = userEvent;
  await user.click(screen.getByRole("button", { name: "Verify email" }));
  await expect.element(screen.getByRole("button", { name: "Verify email" })).toBeDisabled();
  expect(confirmEmailVerification).toHaveBeenCalledTimes(1);
});
