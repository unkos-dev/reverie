import { onlineManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { beforeEach, afterEach, expect, test, vi } from "vite-plus/test";
import { createMemoryRouter, RouterProvider } from "react-router";
import { StrictMode } from "react";

import { ApiError } from "@/api";
import { queryKeys } from "@/lib/query/keys";
import { fetchSetupStatus, lookupCredentialLink, redeemCredentialLink } from "@/api/auth";

import { Component } from "./set-password";
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
    password_min_length: 15,
    password_max_length: 256,
  });
  vi.mocked(lookupCredentialLink).mockResolvedValue({
    email: "bob@example.com",
    has_password: false,
  });
  vi.mocked(redeemCredentialLink).mockResolvedValue(undefined);
});
afterEach(cleanup);

function renderPage(): QueryClient {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const router = createMemoryRouter([{ path: "/set-password", Component, loader }], {
    initialEntries: ["/set-password"],
  });
  render(
    <QueryClientProvider client={client}>
      <RouterProvider router={router} />
    </QueryClientProvider>,
  );
  return client;
}

async function submitPassword(password = "a long chosen passphrase"): Promise<void> {
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText("New password"), password);
  await user.type(screen.getByLabelText("Confirm password"), password);
  await user.click(screen.getByRole("button", { name: "Set password" }));
}

test("checks the link and clears the fragment", async () => {
  vi.mocked(lookupCredentialLink).mockReturnValue(new Promise(() => {}));
  renderPage();
  expect(await screen.findByRole("status", { name: "Checking link" })).toBeInTheDocument();
  expect(window.location.hash).toBe("");
  await waitFor(() => {
    expect(lookupCredentialLink).toHaveBeenCalledWith("test-link", expect.any(AbortSignal));
  });
});

test("clears the fragment before lookup starts, including Strict Mode mounting", async () => {
  const fragments: string[] = [];
  vi.mocked(lookupCredentialLink).mockImplementation(() => {
    fragments.push(window.location.hash);
    return new Promise(() => {});
  });
  const client = new QueryClient();
  const router = createMemoryRouter([{ path: "/set-password", Component, loader }], {
    initialEntries: ["/set-password"],
  });
  render(
    <StrictMode>
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </StrictMode>,
  );
  await waitFor(() => {
    expect(lookupCredentialLink).toHaveBeenCalledWith("test-link", expect.any(AbortSignal));
  });
  expect(fragments.every((fragment) => fragment === "")).toBe(true);
});

test.each([new TypeError("Failed to fetch"), new ApiError(503, null, "Busy", "Try later")])(
  "retries password-policy loading in place after %s",
  async (error) => {
    const status = {
      setup_required: false,
      local_auth_enabled: true,
      oidc_enabled: false,
      password_min_length: 15,
      password_max_length: 256,
    };
    vi.mocked(fetchSetupStatus).mockRejectedValueOnce(error);
    renderPage();
    const user = userEvent.setup();
    const retry = await screen.findByRole("button", { name: "Retry password policy" });
    await user.type(screen.getByLabelText("New password"), "a long chosen passphrase");
    await user.type(screen.getByLabelText("Confirm password"), "a long chosen passphrase");
    expect(screen.getByRole("button", { name: "Set password" })).toBeDisabled();
    let complete: ((value: typeof status) => void) | undefined;
    vi.mocked(fetchSetupStatus).mockReturnValueOnce(
      new Promise((resolve) => {
        complete = resolve;
      }),
    );
    await user.click(retry);
    expect(retry).toBeDisabled();
    expect(screen.queryByText(/Reload to try again/)).not.toBeInTheDocument();
    await act(async () => {
      complete?.(status);
      await Promise.resolve();
    });
    expect(await screen.findByRole("button", { name: "Set password" })).toBeEnabled();
    expect(screen.getByLabelText("New password")).toHaveValue("a long chosen passphrase");
    expect(window.location.hash).toBe("");
    expect(lookupCredentialLink).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Set password" }));
    expect(await screen.findByText("Password set")).toBeInTheDocument();
  },
);

test("keeps a loaded password policy usable during background refresh", async () => {
  const client = renderPage();
  await waitFor(() => {
    expect(screen.getByRole("button", { name: "Set password" })).toBeEnabled();
  });
  vi.mocked(fetchSetupStatus).mockReturnValueOnce(new Promise(() => {}));
  act(() => {
    void client.invalidateQueries({ queryKey: queryKeys.auth.setupStatus() });
  });
  await waitFor(() => {
    expect(fetchSetupStatus).toHaveBeenCalledTimes(2);
  });
  expect(screen.getByText(/Use 15 to 256 characters/)).toBeInTheDocument();
  expect(screen.getByRole("button", { name: "Set password" })).toBeEnabled();
  expect(screen.queryByRole("button", { name: "Retry password policy" })).not.toBeInTheDocument();
});

test("sets a new account password and offers explicit sign in", async () => {
  renderPage();
  expect(await screen.findByText("Set your password")).toBeInTheDocument();
  expect(screen.getByText("Choose a password for bob@example.com.")).toBeInTheDocument();
  expect(document.querySelector('input[autocomplete="username"]')).toHaveValue("bob@example.com");
  await submitPassword();
  expect(redeemCredentialLink).toHaveBeenCalledWith("test-link", "a long chosen passphrase");
  expect(await screen.findByText("Password set")).toBeInTheDocument();
  expect(screen.getByText("You can now sign in with your new password.")).toBeInTheDocument();
  expect(screen.getByRole("link", { name: "Sign in" })).toHaveAttribute("href", "/login");
});

test("successful redemption stays saved after reconnect and a late lookup error", async () => {
  const client = renderPage();
  await submitPassword();
  expect(await screen.findByText("Password set")).toBeInTheDocument();
  vi.mocked(lookupCredentialLink).mockRejectedValue(
    new ApiError(422, null, "Validation failed", "invalid or expired link"),
  );
  await act(async () => {
    onlineManager.setOnline(false);
    onlineManager.setOnline(true);
    await Promise.resolve();
  });
  expect(lookupCredentialLink).toHaveBeenCalledTimes(1);
  const query = client.getQueryCache().find({ queryKey: ["credential-link", "test-link"] });
  expect(query).toBeDefined();
  await act(async () => {
    query?.setState({
      status: "error",
      error: new ApiError(422, null, "Validation failed", "invalid or expired link"),
    });
    await new Promise((resolve) => setTimeout(resolve, 0));
  });
  expect(screen.getByText("Password set")).toBeInTheDocument();
  expect(screen.queryByText("This link can't be used")).not.toBeInTheDocument();
});

test("does not offer retry for a permanent lookup refusal", async () => {
  vi.mocked(lookupCredentialLink).mockRejectedValue(
    new ApiError(403, null, "Forbidden", "Access refused"),
  );
  renderPage();
  expect(await screen.findByRole("alert")).toHaveTextContent("Access refused");
  expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
});

test.each([new TypeError("Failed to fetch"), new ApiError(503, null, "Busy", "Try later")])(
  "reconnect preserves entered passwords when lookup would fail: %s",
  async (error) => {
    renderPage();
    const user = userEvent.setup();
    const password = await screen.findByLabelText("New password");
    const confirm = screen.getByLabelText("Confirm password");
    await user.type(password, "a long chosen passphrase");
    await user.type(confirm, "a long chosen passphrase");
    vi.mocked(lookupCredentialLink).mockRejectedValue(error);
    await act(async () => {
      onlineManager.setOnline(false);
      onlineManager.setOnline(true);
      await new Promise((resolve) => setTimeout(resolve, 0));
    });
    expect(lookupCredentialLink).toHaveBeenCalledTimes(1);
    expect(password).toBeInTheDocument();
    expect(confirm).toBeInTheDocument();
    expect(password).toHaveValue("a long chosen passphrase");
    expect(confirm).toHaveValue("a long chosen passphrase");
  },
);

test.each([
  new TypeError("Failed to fetch"),
  new ApiError(503, null, "Busy", "Try later"),
  new ApiError(429, null, "Limited", "Try later"),
])("retries a temporary lookup failure without restoring the fragment: %s", async (error) => {
  vi.mocked(lookupCredentialLink).mockRejectedValueOnce(error);
  renderPage();
  const user = userEvent.setup();
  const retry = await screen.findByRole("button", { name: "Try again" });
  let complete: ((account: { email: string; has_password: boolean }) => void) | undefined;
  vi.mocked(lookupCredentialLink).mockReturnValueOnce(
    new Promise((resolve) => {
      complete = resolve;
    }),
  );
  await user.click(retry);
  expect(retry).toBeDisabled();
  expect(window.location.hash).toBe("");
  expect(redeemCredentialLink).not.toHaveBeenCalled();
  await act(async () => {
    complete?.({ email: "bob@example.com", has_password: false });
    await Promise.resolve();
  });
  expect(await screen.findByLabelText("New password")).toBeInTheDocument();
  await waitFor(() => {
    expect(lookupCredentialLink).toHaveBeenCalledTimes(2);
  });
  expect(lookupCredentialLink).toHaveBeenLastCalledWith("test-link", expect.any(AbortSignal));
});

test("shows reset copy for an account with a password", async () => {
  vi.mocked(lookupCredentialLink).mockResolvedValue({
    email: "bob@example.com",
    has_password: true,
  });
  renderPage();
  expect(await screen.findByText("Reset your password")).toBeInTheDocument();
  expect(
    screen.getByText(
      "Choose a new password for bob@example.com. Saving it signs you out everywhere.",
    ),
  ).toBeInTheDocument();
});

test.each(["unknown", "expired", "used", "disabled", "no email"])(
  "shows the same invalid state for %s links",
  async () => {
    vi.mocked(lookupCredentialLink).mockRejectedValue(
      new ApiError(422, null, "Validation failed", "invalid or expired link"),
    );
    renderPage();
    expect(await screen.findByText("This link can't be used")).toBeInTheDocument();
    expect(
      screen.getByText(
        "It may have expired or already been used. Ask your administrator for a new link.",
      ),
    ).toBeInTheDocument();
    expect(screen.getByRole("link", { name: "Go to sign in" })).toHaveAttribute("href", "/login");
    expect(screen.queryByRole("button", { name: "Try again" })).not.toBeInTheDocument();
  },
);

test("a missing fragment is invalid without a lookup", async () => {
  window.history.replaceState(null, "", "/set-password");
  renderPage();
  expect(await screen.findByText("This link can't be used")).toBeInTheDocument();
  expect(lookupCredentialLink).not.toHaveBeenCalled();
});

test("rejects mismatched passwords without redemption", async () => {
  renderPage();
  const user = userEvent.setup();
  await user.type(await screen.findByLabelText("New password"), "a long chosen passphrase");
  await user.type(screen.getByLabelText("Confirm password"), "different passphrase");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(screen.getByLabelText("Confirm password")).toHaveAccessibleDescription(
    "The passwords don't match.",
  );
  expect(screen.getByLabelText("Confirm password")).toHaveFocus();
  expect(screen.getByLabelText("New password")).not.toHaveAttribute("aria-invalid");
  expect(redeemCredentialLink).not.toHaveBeenCalled();
});

test("confirmation is checked again after the original password changes", async () => {
  renderPage();
  const user = userEvent.setup();
  const password = await screen.findByLabelText("New password");
  const confirm = screen.getByLabelText("Confirm password");
  await user.type(password, "a long chosen passphrase");
  await user.type(confirm, "a long chosen passphrase");
  await user.type(password, "!");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(confirm).toHaveAccessibleDescription("The passwords don't match.");
  expect(redeemCredentialLink).not.toHaveBeenCalled();

  await user.type(confirm, "!");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(redeemCredentialLink).toHaveBeenCalledExactlyOnceWith(
    "test-link",
    "a long chosen passphrase!",
  );
});

test("a shown mismatch does not block once the original password is corrected to match", async () => {
  renderPage();
  const user = userEvent.setup();
  const password = await screen.findByLabelText("New password");
  const confirm = screen.getByLabelText("Confirm password");
  await user.type(password, "a long chosen passphrase!");
  await user.type(confirm, "a long chosen passphrase");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(confirm).toHaveAccessibleDescription("The passwords don't match.");

  await user.type(password, "{Backspace}");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(redeemCredentialLink).toHaveBeenCalledExactlyOnceWith(
    "test-link",
    "a long chosen passphrase",
  );
});

test("enforces the existing password policy", async () => {
  renderPage();
  await submitPassword("short");
  expect(screen.getByLabelText("New password")).toHaveAccessibleDescription(
    /Use at least 15 characters\./,
  );
  expect(screen.getByLabelText("New password")).toHaveFocus();
  expect(screen.getByLabelText("Confirm password")).not.toHaveAttribute("aria-invalid");
  expect(redeemCredentialLink).not.toHaveBeenCalled();
});

test("counts the new password in Unicode scalar values", async () => {
  renderPage();
  const user = userEvent.setup();
  const password = await screen.findByLabelText("New password");
  const confirm = screen.getByLabelText("Confirm password");
  await user.click(password);
  await user.paste("🙂".repeat(14));
  await user.click(confirm);
  await user.paste("🙂".repeat(14));
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(password).toHaveAccessibleDescription(/Use at least 15 characters\./);
  expect(redeemCredentialLink).not.toHaveBeenCalled();

  await user.click(password);
  await user.paste("🙂");
  await user.click(confirm);
  await user.paste("🙂");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(redeemCredentialLink).toHaveBeenCalledExactlyOnceWith("test-link", "🙂".repeat(15));
});

test("a refused redemption is a general alert that keeps the values and marks no field invalid", async () => {
  vi.mocked(redeemCredentialLink).mockRejectedValue(
    new ApiError(429, null, "Request failed", "Please try again later."),
  );
  renderPage();
  await submitPassword();
  expect(await screen.findByRole("alert")).toHaveTextContent("Please try again later.");
  expect(screen.getByLabelText("New password")).toHaveValue("a long chosen passphrase");
  expect(screen.getByLabelText("Confirm password")).toHaveValue("a long chosen passphrase");
  expect(screen.getByLabelText("New password")).not.toHaveAttribute("aria-invalid");
  expect(screen.getByLabelText("Confirm password")).not.toHaveAttribute("aria-invalid");
});

test("repeated click and Enter while redemption is pending send one request", async () => {
  vi.mocked(redeemCredentialLink).mockReturnValue(new Promise(() => {}));
  renderPage();
  await submitPassword();
  const user = userEvent.setup();
  await user.click(screen.getByRole("button", { name: "Set password" }));
  await user.keyboard("{Enter}");
  expect(redeemCredentialLink).toHaveBeenCalledTimes(1);
});

test.each([422, 429, 503])("renders the redemption detail for %s", async (status) => {
  vi.mocked(redeemCredentialLink).mockRejectedValue(
    new ApiError(status, null, "Request failed", "Please try again later."),
  );
  renderPage();
  await submitPassword();
  expect(await screen.findByRole("alert")).toHaveTextContent("Please try again later.");
});

test("a lost redemption response explains possible success without retrying or clearing the form", async () => {
  vi.mocked(redeemCredentialLink).mockRejectedValue(new TypeError("Failed to fetch"));
  renderPage();
  await submitPassword();
  expect(await screen.findByRole("alert")).toHaveTextContent(
    "Your password may have changed. Try signing in with the password you entered before submitting again.",
  );
  expect(screen.getByRole("link", { name: "Try signing in" })).toHaveAttribute("href", "/login");
  expect(screen.getByLabelText("New password")).toHaveValue("a long chosen passphrase");
  expect(screen.getByLabelText("Confirm password")).toHaveValue("a long chosen passphrase");
  expect(redeemCredentialLink).toHaveBeenCalledTimes(1);
  expect(screen.queryByText("Password set")).not.toBeInTheDocument();
  expect(screen.queryByText("This link can't be used")).not.toBeInTheDocument();
});

test.each([429, 503])("renders the lookup detail for %s", async (status) => {
  vi.mocked(lookupCredentialLink).mockRejectedValue(
    new ApiError(status, null, "Request failed", "Please try again later."),
  );
  renderPage();
  expect(await screen.findByRole("alert")).toHaveTextContent("Please try again later.");
});

test("an invalid redemption replaces the form with the invalid state", async () => {
  vi.mocked(redeemCredentialLink).mockRejectedValue(
    new ApiError(422, null, "Validation failed", "invalid or expired link"),
  );
  renderPage();
  await submitPassword();
  expect(await screen.findByText("This link can't be used")).toBeInTheDocument();
});

test("marks the action pending while redemption is in flight", async () => {
  vi.mocked(redeemCredentialLink).mockReturnValue(new Promise(() => {}));
  renderPage();
  await submitPassword();
  expect(screen.getByRole("button", { name: "Set password" })).toHaveAttribute("data-pending");
});
