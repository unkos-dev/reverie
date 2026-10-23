import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, screen } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, expect, test, vi } from "vite-plus/test";
import { MemoryRouter } from "react-router";
import { ApiError } from "@/api";
import { fetchEmailVerification, requestEmailVerification } from "@/api/auth";
import { Component } from "./account-email";

vi.mock("@/api/auth");
vi.mock("@/lib/theme/ThemeProvider", () => ({ useTheme: () => ({ effective: "dark" }) }));
function page(): void {
  render(
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
  page();
  const user = userEvent.setup();
  expect(await screen.findByText("holder@example.test")).toBeInTheDocument();
  expect(screen.getByText("Not verified")).toBeInTheDocument();
  expect(requestEmailVerification).not.toHaveBeenCalled();
  await user.click(screen.getByRole("button", { name: "Send verification email" }));
  expect(await screen.findByText(/Verification email requested/)).toBeInTheDocument();
  expect(screen.getByRole("heading", { name: "Email address" })).toHaveFocus();
  await user.click(screen.getByRole("button", { name: "Send again" }));
  expect(requestEmailVerification).toHaveBeenCalledTimes(2);
  expect(screen.getByRole("link", { name: "Back to library" })).toHaveAttribute("href", "/library");
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
  page();
  expect(await screen.findByText(copy)).toBeInTheDocument();
  expect(screen.queryByRole("button")).not.toBeInTheDocument();
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
  page();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Send verification email" }));
  expect(await screen.findByRole("alert")).toHaveTextContent(copy);
  expect(screen.getByRole("button", { name: "Send verification email" })).toBeEnabled();
});
test("keeps submission disabled while its request is pending", async () => {
  vi.mocked(requestEmailVerification).mockReturnValue(new Promise(() => {}));
  page();
  const user = userEvent.setup();
  await user.click(await screen.findByRole("button", { name: "Send verification email" }));
  expect(screen.getByRole("button", { name: "Send verification email" })).toBeDisabled();
});
test("retries a failed status query", async () => {
  vi.mocked(fetchEmailVerification).mockRejectedValueOnce(new Error("offline"));
  page();
  const user = userEvent.setup();
  expect(await screen.findByRole("alert")).toHaveTextContent("Could not load your email settings.");
  await user.click(screen.getByRole("button", { name: "Try again" }));
  expect(await screen.findByText("holder@example.test")).toBeInTheDocument();
  expect(requestEmailVerification).not.toHaveBeenCalled();
});
