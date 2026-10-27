import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { beforeEach, expect, test, vi } from "vite-plus/test";
import { MemoryRouter } from "react-router";
import { ApiError } from "@/api";
import { fetchEmailVerification, requestEmailVerification } from "@/api/auth";
import { Component } from "@/routes/account-email";

vi.mock("@/api/auth");
vi.mock("@/lib/theme/ThemeProvider", () => ({ useTheme: () => ({ effective: "dark" }) }));
async function page(): Promise<void> {
  await render(
    <QueryClientProvider
      client={new QueryClient({ defaultOptions: { queries: { retry: false } } })}
    >
      <MemoryRouter>
        <Component />
      </MemoryRouter>
    </QueryClientProvider>,
  );
}
beforeEach(() => {
  vi.resetAllMocks();
  vi.mocked(fetchEmailVerification).mockResolvedValue({
    email: "holder@example.test",
    email_verified: false,
    mail_enabled: true,
  });
  vi.mocked(requestEmailVerification).mockResolvedValue(undefined);
});
test("requests verification explicitly and permits a repeat request", async () => {
  await page();
  const user = userEvent;
  await expect.element(screen.getByText("holder@example.test")).toBeInTheDocument();
  await expect.element(screen.getByText("Not verified")).toBeInTheDocument();
  expect(requestEmailVerification).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Send verification email" }));
  await expect.element(screen.getByText(/Verification email requested/)).toBeInTheDocument();
  await expect.element(screen.getByRole("heading", { name: "Email address" })).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "Send again" }));
  expect(requestEmailVerification).toHaveBeenCalledTimes(2);
  await expect
    .element(screen.getByRole("link", { name: "Back to library" }))
    .toHaveAttribute("href", "/library");
});
test.each([
  [
    { email: null, email_verified: false, mail_enabled: true },
    "Your account has no email address. Ask your administrator to add one.",
  ],
  [
    { email: "holder@example.test", email_verified: true, mail_enabled: false },
    "Your email address is verified.",
  ],
  [
    { email: "holder@example.test", email_verified: false, mail_enabled: false },
    "Email isn't set up on this server, so this address can't be verified yet.",
  ],
])("renders an inactive request state", async (status, copy) => {
  vi.mocked(fetchEmailVerification).mockResolvedValue(status);
  await page();
  await expect.element(screen.getByText(copy)).toBeInTheDocument();
  await expect.element(screen.getByRole("button")).not.toBeInTheDocument();
  expect(requestEmailVerification).not.toHaveBeenCalled();
});
test.each([
  [
    new ApiError(503, null, "Unavailable", "private"),
    "Email isn't available right now. Try again later.",
  ],
  [new ApiError(429, null, "Wait", "Wait a minute."), "Wait a minute."],
  [new TypeError("offline"), "Could not confirm the request. Try again."],
])("reports a request failure without claiming a send", async (error, copy) => {
  vi.mocked(requestEmailVerification).mockRejectedValue(error);
  await page();
  const user = userEvent;
  await user.click(screen.getByRole("button", { name: "Send verification email" }));
  await expect.element(screen.getByRole("alert")).toHaveTextContent(copy);
  await expect
    .element(screen.getByRole("button", { name: "Send verification email" }))
    .toBeEnabled();
});
test("keeps submission disabled while its request is pending", async () => {
  vi.mocked(requestEmailVerification).mockReturnValue(new Promise(() => {}));
  await page();
  const user = userEvent;
  await user.click(screen.getByRole("button", { name: "Send verification email" }));
  await expect
    .element(screen.getByRole("button", { name: "Send verification email" }))
    .toBeDisabled();
});
test("retries a failed status query", async () => {
  vi.mocked(fetchEmailVerification).mockRejectedValueOnce(new Error("offline"));
  await page();
  const user = userEvent;
  await expect
    .element(screen.getByRole("alert"))
    .toHaveTextContent("Could not load your email settings.");
  await user.click(screen.getByRole("button", { name: "Try again" }));
  await expect.element(screen.getByText("holder@example.test")).toBeInTheDocument();
  expect(requestEmailVerification).not.toHaveBeenCalled();
});
