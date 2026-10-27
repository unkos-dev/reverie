import { onlineManager, QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { render, cleanup } from "vitest-browser-react";
import { page as screen, userEvent } from "vite-plus/test/browser";
import { beforeEach, afterEach, expect, test, vi } from "vite-plus/test";
import { createMemoryRouter, RouterProvider } from "react-router";
import { StrictMode } from "react";

import { ApiError } from "@/api";
import { queryKeys } from "@/lib/query/keys";
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
  vi.mocked(lookupCredentialLink).mockResolvedValue({
    email: "bob@example.com",
    has_password: false,
  });
  vi.mocked(redeemCredentialLink).mockResolvedValue(undefined);
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

async function submitPassword(password = "a long chosen passphrase"): Promise<void> {
  const user = userEvent;
  await user.type(screen.getByLabelText("New password"), password);
  await user.type(screen.getByLabelText("Confirm password"), password);
  await user.click(screen.getByRole("button", { name: "Set password" }));
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
  await render(
    <StrictMode>
      <QueryClientProvider client={client}>
        <RouterProvider router={router} />
      </QueryClientProvider>
    </StrictMode>,
  );
  await vi.waitFor(() => {
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
      email_recovery_enabled: false,
      password_min_length: 15,
      password_max_length: 256,
    };
    vi.mocked(fetchSetupStatus).mockRejectedValueOnce(error);
    await renderPage();
    const user = userEvent;
    const retry = screen.getByRole("button", { name: "Retry password policy" });
    await user.type(screen.getByLabelText("New password"), "a long chosen passphrase");
    await user.type(screen.getByLabelText("Confirm password"), "a long chosen passphrase");
    await expect.element(screen.getByRole("button", { name: "Set password" })).toBeDisabled();
    let complete: ((value: typeof status) => void) | undefined;
    vi.mocked(fetchSetupStatus).mockReturnValueOnce(
      new Promise((resolve) => {
        complete = resolve;
      }),
    );
    await user.click(retry);
    await expect.element(retry).toBeDisabled();
    await expect.element(screen.getByText(/Reload to try again/)).not.toBeInTheDocument();

    complete?.(status);
    await Promise.resolve();

    await expect.element(screen.getByRole("button", { name: "Set password" })).toBeEnabled();
    await expect
      .element(screen.getByLabelText("New password"))
      .toHaveValue("a long chosen passphrase");
    expect(window.location.hash).toBe("");
    expect(lookupCredentialLink).toHaveBeenCalledTimes(1);
    await user.click(screen.getByRole("button", { name: "Set password" }));
    await expect.element(screen.getByText("Password set")).toBeInTheDocument();
  },
);

test("keeps a loaded password policy usable during background refresh", async () => {
  const client = await renderPage();
  await vi.waitFor(async () => {
    await expect.element(screen.getByRole("button", { name: "Set password" })).toBeEnabled();
  });
  vi.mocked(fetchSetupStatus).mockReturnValueOnce(new Promise(() => {}));

  void client.invalidateQueries({ queryKey: queryKeys.auth.setupStatus() });

  await vi.waitFor(() => {
    expect(fetchSetupStatus).toHaveBeenCalledTimes(2);
  });
  await expect.element(screen.getByText(/Use 15 to 256 characters/)).toBeInTheDocument();
  await expect.element(screen.getByRole("button", { name: "Set password" })).toBeEnabled();
  await expect
    .element(screen.getByRole("button", { name: "Retry password policy" }))
    .not.toBeInTheDocument();
});

test("sets a new account password and offers explicit sign in", async () => {
  await renderPage();
  await expect.element(screen.getByText("Set your password")).toBeInTheDocument();
  await expect
    .element(screen.getByText("Choose a password for bob@example.com."))
    .toBeInTheDocument();
  await expect
    .element(document.querySelector<HTMLElement>('input[autocomplete="username"]'))
    .toHaveValue("bob@example.com");
  await submitPassword();
  expect(redeemCredentialLink).toHaveBeenCalledWith("test-link", "a long chosen passphrase");
  await expect.element(screen.getByText("Password set")).toBeInTheDocument();
  await expect
    .element(screen.getByText("You can now sign in with your new password."))
    .toBeInTheDocument();
  await expect
    .element(screen.getByRole("link", { name: "Sign in" }))
    .toHaveAttribute("href", "/login");
});

test("successful redemption stays saved after reconnect and a late lookup error", async () => {
  const client = await renderPage();
  await submitPassword();
  await expect.element(screen.getByText("Password set")).toBeInTheDocument();
  vi.mocked(lookupCredentialLink).mockRejectedValue(
    new ApiError(422, null, "Validation failed", "invalid or expired link"),
  );

  onlineManager.setOnline(false);
  onlineManager.setOnline(true);
  await Promise.resolve();

  expect(lookupCredentialLink).toHaveBeenCalledTimes(1);
  const query = client.getQueryCache().find({ queryKey: ["credential-link", "test-link"] });
  expect(query).toBeDefined();

  query?.setState({
    status: "error",
    error: new ApiError(422, null, "Validation failed", "invalid or expired link"),
  });
  await new Promise((resolve) => setTimeout(resolve, 0));

  await expect.element(screen.getByText("Password set")).toBeInTheDocument();
  await expect.element(screen.getByText("This link can't be used")).not.toBeInTheDocument();
});

test("does not offer retry for a permanent lookup refusal", async () => {
  vi.mocked(lookupCredentialLink).mockRejectedValue(
    new ApiError(403, null, "Forbidden", "Access refused"),
  );
  await renderPage();
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Access refused");
  await expect.element(screen.getByRole("button", { name: "Try again" })).not.toBeInTheDocument();
});

test.each([new TypeError("Failed to fetch"), new ApiError(503, null, "Busy", "Try later")])(
  "reconnect preserves entered passwords when lookup would fail: %s",
  async (error) => {
    await renderPage();
    const user = userEvent;
    const password = screen.getByLabelText("New password");
    const confirm = screen.getByLabelText("Confirm password");
    await user.type(password, "a long chosen passphrase");
    await user.type(confirm, "a long chosen passphrase");
    vi.mocked(lookupCredentialLink).mockRejectedValue(error);

    onlineManager.setOnline(false);
    onlineManager.setOnline(true);
    await new Promise((resolve) => setTimeout(resolve, 0));

    expect(lookupCredentialLink).toHaveBeenCalledTimes(1);
    await expect.element(password).toBeInTheDocument();
    await expect.element(confirm).toBeInTheDocument();
    await expect.element(password).toHaveValue("a long chosen passphrase");
    await expect.element(confirm).toHaveValue("a long chosen passphrase");
  },
);

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

test("shows reset copy for an account with a password", async () => {
  vi.mocked(lookupCredentialLink).mockResolvedValue({
    email: "bob@example.com",
    has_password: true,
  });
  await renderPage();
  await expect.element(screen.getByText("Reset your password")).toBeInTheDocument();
  await expect
    .element(
      screen.getByText(
        "Choose a new password for bob@example.com. Saving it signs you out everywhere.",
      ),
    )
    .toBeInTheDocument();
});

test.each(["unknown", "expired", "used", "disabled", "no email"])(
  "shows the same invalid state for %s links",
  async () => {
    vi.mocked(lookupCredentialLink).mockRejectedValue(
      new ApiError(422, null, "Validation failed", "invalid or expired link"),
    );
    await renderPage();
    await expect.element(screen.getByText("This link can't be used")).toBeInTheDocument();
    await expect
      .element(
        screen.getByText(
          "It may have expired or already been used. Ask your administrator for a new link.",
        ),
      )
      .toBeInTheDocument();
    await expect
      .element(screen.getByRole("link", { name: "Go to sign in" }))
      .toHaveAttribute("href", "/login");
    await expect.element(screen.getByRole("button", { name: "Try again" })).not.toBeInTheDocument();
  },
);

test("a missing fragment is invalid without a lookup", async () => {
  window.history.replaceState(null, "", "/set-password");
  await renderPage();
  await expect.element(screen.getByText("This link can't be used")).toBeInTheDocument();
  expect(lookupCredentialLink).not.toHaveBeenCalled();
});

test("rejects mismatched passwords without redemption", async () => {
  await renderPage();
  const user = userEvent;
  await user.type(screen.getByLabelText("New password"), "a long chosen passphrase");
  await user.type(screen.getByLabelText("Confirm password"), "different passphrase");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  await expect
    .element(screen.getByLabelText("Confirm password"))
    .toHaveAccessibleDescription("The passwords don't match.");
  await expect.element(screen.getByLabelText("Confirm password")).toHaveFocus();
  await expect.element(screen.getByLabelText("New password")).not.toHaveAttribute("aria-invalid");
  expect(redeemCredentialLink).not.toHaveBeenCalled();
});

test("confirmation is checked again after the original password changes", async () => {
  await renderPage();
  const user = userEvent;
  const password = screen.getByLabelText("New password");
  const confirm = screen.getByLabelText("Confirm password");
  await user.type(password, "a long chosen passphrase");
  await user.type(confirm, "a long chosen passphrase");
  await user.type(password, "!");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  await expect.element(confirm).toHaveAccessibleDescription("The passwords don't match.");
  expect(redeemCredentialLink).not.toHaveBeenCalled();

  await user.type(confirm, "!");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(redeemCredentialLink).toHaveBeenCalledExactlyOnceWith(
    "test-link",
    "a long chosen passphrase!",
  );
});

test("a shown mismatch does not block once the original password is corrected to match", async () => {
  await renderPage();
  const user = userEvent;
  const password = screen.getByLabelText("New password");
  const confirm = screen.getByLabelText("Confirm password");
  await user.type(password, "a long chosen passphrase!");
  await user.type(confirm, "a long chosen passphrase");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  await expect.element(confirm).toHaveAccessibleDescription("The passwords don't match.");

  await user.type(password, "{Backspace}");
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(redeemCredentialLink).toHaveBeenCalledExactlyOnceWith(
    "test-link",
    "a long chosen passphrase",
  );
});

test("enforces the existing password policy", async () => {
  await renderPage();
  await submitPassword("short");
  await expect
    .element(screen.getByLabelText("New password"))
    .toHaveAccessibleDescription(/Use at least 15 characters\./);
  await expect.element(screen.getByLabelText("New password")).toHaveFocus();
  await expect
    .element(screen.getByLabelText("Confirm password"))
    .not.toHaveAttribute("aria-invalid");
  expect(redeemCredentialLink).not.toHaveBeenCalled();
});

test("counts the new password in Unicode scalar values", async () => {
  await renderPage();
  const user = userEvent;
  const password = screen.getByLabelText("New password");
  const confirm = screen.getByLabelText("Confirm password");
  await user.fill(password, "🙂".repeat(14));
  await user.fill(confirm, "🙂".repeat(14));
  await user.click(screen.getByRole("button", { name: "Set password" }));
  await expect.element(password).toHaveAccessibleDescription(/Use at least 15 characters\./);
  expect(redeemCredentialLink).not.toHaveBeenCalled();

  await user.fill(password, "🙂".repeat(15));
  await user.fill(confirm, "🙂".repeat(15));
  await user.click(screen.getByRole("button", { name: "Set password" }));
  expect(redeemCredentialLink).toHaveBeenCalledExactlyOnceWith("test-link", "🙂".repeat(15));
});

test("a refused redemption is a general alert that keeps the values and marks no field invalid", async () => {
  vi.mocked(redeemCredentialLink).mockRejectedValue(
    new ApiError(429, null, "Request failed", "Please try again later."),
  );
  await renderPage();
  await submitPassword();
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Please try again later.");
  await expect
    .element(screen.getByLabelText("New password"))
    .toHaveValue("a long chosen passphrase");
  await expect
    .element(screen.getByLabelText("Confirm password"))
    .toHaveValue("a long chosen passphrase");
  await expect.element(screen.getByLabelText("New password")).not.toHaveAttribute("aria-invalid");
  await expect
    .element(screen.getByLabelText("Confirm password"))
    .not.toHaveAttribute("aria-invalid");
});

test("repeated click and Enter while redemption is pending send one request", async () => {
  let rejectRequest: ((reason: Error) => void) | undefined;
  vi.mocked(redeemCredentialLink).mockReturnValue(
    new Promise<void>((_resolve, reject) => {
      rejectRequest = reject;
    }),
  );
  await renderPage();
  await submitPassword();
  const user = userEvent;
  await user.click(screen.getByRole("button", { name: "Set password" }), { force: true });
  await user.keyboard("{Enter}");
  expect(redeemCredentialLink).toHaveBeenCalledTimes(1);
  rejectRequest?.(new ApiError(503, null, "Unavailable", "Request refused."));
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Request refused.");
  await expect
    .element(screen.getByLabelText("New password"))
    .toHaveValue("a long chosen passphrase");
  await expect
    .element(screen.getByLabelText("Confirm password"))
    .toHaveValue("a long chosen passphrase");
  await expect
    .element(screen.getByRole("button", { name: "Set password" }))
    .not.toHaveAttribute("data-pending");
  vi.mocked(redeemCredentialLink).mockResolvedValue(undefined);
  await userEvent.click(screen.getByRole("button", { name: "Set password" }));
  await expect.element(screen.getByText("Password set")).toBeInTheDocument();
  expect(redeemCredentialLink).toHaveBeenCalledTimes(2);
});

test.each([422, 429, 503])("renders the redemption detail for %s", async (status) => {
  vi.mocked(redeemCredentialLink).mockRejectedValue(
    new ApiError(status, null, "Request failed", "Please try again later."),
  );
  await renderPage();
  await submitPassword();
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Please try again later.");
});

test("a lost redemption response explains possible success without retrying or clearing the form", async () => {
  vi.mocked(redeemCredentialLink).mockRejectedValue(new TypeError("Failed to fetch"));
  await renderPage();
  await submitPassword();
  await expect
    .element(screen.getByRole("alert"))
    .toHaveTextContent(
      "Your password may have changed. Try signing in with the password you entered before submitting again.",
    );
  await expect
    .element(screen.getByRole("link", { name: "Try signing in" }))
    .toHaveAttribute("href", "/login");
  await expect
    .element(screen.getByLabelText("New password"))
    .toHaveValue("a long chosen passphrase");
  await expect
    .element(screen.getByLabelText("Confirm password"))
    .toHaveValue("a long chosen passphrase");
  expect(redeemCredentialLink).toHaveBeenCalledTimes(1);
  await expect.element(screen.getByText("Password set")).not.toBeInTheDocument();
  await expect.element(screen.getByText("This link can't be used")).not.toBeInTheDocument();
});

test.each([429, 503])("renders the lookup detail for %s", async (status) => {
  vi.mocked(lookupCredentialLink).mockRejectedValue(
    new ApiError(status, null, "Request failed", "Please try again later."),
  );
  await renderPage();
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Please try again later.");
});

test("an invalid redemption replaces the form with the invalid state", async () => {
  vi.mocked(redeemCredentialLink).mockRejectedValue(
    new ApiError(422, null, "Validation failed", "invalid or expired link"),
  );
  await renderPage();
  await submitPassword();
  await expect.element(screen.getByText("This link can't be used")).toBeInTheDocument();
});

test("marks the action pending while redemption is in flight", async () => {
  let rejectRequest: ((reason: Error) => void) | undefined;
  vi.mocked(redeemCredentialLink).mockReturnValue(
    new Promise<void>((_resolve, reject) => {
      rejectRequest = reject;
    }),
  );
  await renderPage();
  await submitPassword();
  await expect
    .element(screen.getByRole("button", { name: "Set password" }))
    .toHaveAttribute("data-pending");
  await vi.waitFor(async () => {
    await expect
      .element(
        document.querySelector<HTMLElement>('[aria-live="assertive"] [role="img"]:last-child'),
      )
      .toHaveAccessibleName("Set password");
  });
  rejectRequest?.(new ApiError(503, null, "Unavailable", "Request refused."));
  await expect.element(screen.getByRole("alert")).toHaveTextContent("Request refused.");
  await expect
    .element(screen.getByLabelText("New password"))
    .toHaveValue("a long chosen passphrase");
  await expect
    .element(screen.getByLabelText("Confirm password"))
    .toHaveValue("a long chosen passphrase");
  await expect
    .element(screen.getByRole("button", { name: "Set password" }))
    .not.toHaveAttribute("data-pending");
  vi.mocked(redeemCredentialLink).mockResolvedValue(undefined);
  await userEvent.click(screen.getByRole("button", { name: "Set password" }));
  await expect.element(screen.getByText("Password set")).toBeInTheDocument();
  expect(redeemCredentialLink).toHaveBeenCalledTimes(2);
});

test.each(["/set-password", "/reset-password"])(
  "%s renders from password state rather than its route",
  async (path) => {
    vi.mocked(lookupCredentialLink).mockResolvedValue({
      email: "bob@example.com",
      has_password: true,
    });
    await renderPage(path);
    await expect.element(screen.getByText("Reset your password")).toBeInTheDocument();
    expect(window.location.hash).toBe("");
    expect(redeemCredentialLink).not.toHaveBeenCalled();
  },
);
test.each(["/set-password", "/reset-password"])(
  "%s supports an account with no password",
  async (path) => {
    await renderPage(path);
    await expect.element(screen.getByText("Set your password")).toBeInTheDocument();
  },
);
test.each([
  ["/set-password", true, false],
  ["/reset-password", false, false],
  ["/reset-password", true, true],
])("%s uses its approved invalid-link copy", async (path, enabled, recoveryAction) => {
  vi.mocked(fetchSetupStatus).mockResolvedValue({
    setup_required: false,
    local_auth_enabled: true,
    oidc_enabled: false,
    email_recovery_enabled: enabled,
    password_min_length: 15,
    password_max_length: 256,
  });
  vi.mocked(lookupCredentialLink).mockRejectedValue(
    new ApiError(422, null, "Invalid", "invalid or expired link"),
  );
  await renderPage(path);
  await expect.element(screen.getByText("This link can't be used")).toBeInTheDocument();
  if (recoveryAction)
    await expect
      .element(screen.getByRole("link", { name: "Request a new link" }))
      .toHaveAttribute("href", "/forgot-password");
  else
    await expect
      .element(screen.getByRole("link", { name: "Request a new link" }))
      .not.toBeInTheDocument();
  await expect
    .element(screen.getByRole("link", { name: "Go to sign in" }))
    .toHaveAttribute("href", "/login");
});
