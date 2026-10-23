import { useMutation, useQuery, useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactElement } from "react";
import { Link, useLoaderData } from "react-router";
import { z } from "zod";

import { ApiError } from "@/api";
import { confirmEmailVerification, fetchEmailVerification } from "@/api/auth";
import { emailProofTokenField } from "@/api/auth.schemas";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";
import { AuthShell } from "./auth-shell";

type VerificationView = "verified" | "invalid" | "signed-out" | "loading" | "ready";

function verificationView(
  verified: boolean,
  invalid: boolean,
  signedOut: boolean,
  loaded: boolean,
): VerificationView {
  if (verified) return "verified";
  if (invalid) return "invalid";
  if (signedOut) return "signed-out";
  return loaded ? "ready" : "loading";
}

const verificationCopy = {
  verified: { heading: "Email address verified", body: "Your email address is verified." },
  invalid: {
    heading: "This link can't be used",
    body: "It may have expired, already been used, or belong to a different account. Send a new verification email from your email settings.",
  },
  "signed-out": {
    heading: "Sign in to verify your email",
    body: "Sign in to the account this email was sent to, then open the link in the email again.",
  },
  ready: { heading: "Verify your email address", body: "" },
  loading: { heading: "", body: "" },
};

function VerificationActions({
  view,
  error,
  pending,
  onVerify,
}: Readonly<{
  view: VerificationView;
  error: string | null;
  pending: boolean;
  onVerify: () => void;
}>): ReactElement {
  if (view === "signed-out")
    return (
      <Link to="/login" className="hover:text-fg underline underline-offset-4">
        Go to sign in
      </Link>
    );
  if (view !== "ready")
    return (
      <Link to="/account/email" className="hover:text-fg underline underline-offset-4">
        Go to email settings
      </Link>
    );
  return (
    <>
      {error !== null ? (
        <FieldError>
          {error === "ambiguous" ? (
            <>
              Your email address may already be verified.{" "}
              <Link to="/account/email" className="underline underline-offset-4">
                Check your email settings
              </Link>{" "}
              before trying again.
            </>
          ) : (
            error
          )}
        </FieldError>
      ) : null}
      <Button className="w-full" isDisabled={pending} onPress={onVerify}>
        Verify email
      </Button>
    </>
  );
}

export function Component(): ReactElement {
  const token = z.string().parse(useLoaderData());
  const usable = emailProofTokenField.safeParse(token).success;
  const [verified, setVerified] = useState(false);
  const [invalid, setInvalid] = useState(false);
  const [signedOut, setSignedOut] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const client = useQueryClient();
  const status = useQuery({
    queryKey: queryKeys.auth.emailVerification(),
    queryFn: ({ signal }) => fetchEmailVerification(signal),
    retry: false,
    enabled: usable && !verified && !invalid && !signedOut,
    refetchOnWindowFocus: false,
    refetchOnReconnect: false,
    throwOnError: (cause) =>
      !(cause instanceof ApiError && (cause.status === 401 || cause.status === 403)),
  });
  const unauthenticated =
    signedOut ||
    (status.error instanceof ApiError &&
      (status.error.status === 401 || status.error.status === 403));
  const view = verificationView(
    verified,
    !usable || invalid || status.data?.email === null,
    unauthenticated,
    status.data !== undefined,
  );
  useEffect(() => {
    if (view !== "loading") title.current?.focus();
  }, [view]);
  const confirmation = useMutation({
    gcTime: 0,
    mutationFn: () => confirmEmailVerification(token),
    onSuccess: () => {
      setVerified(true);
      setError(null);
      void client.invalidateQueries({ queryKey: queryKeys.auth.emailVerification() });
    },
    onError: (cause) => {
      if (cause instanceof ApiError && cause.status === 422) setInvalid(true);
      else if (cause instanceof ApiError && (cause.status === 401 || cause.status === 403))
        setSignedOut(true);
      else setError(cause instanceof ApiError && cause.status === 429 ? cause.detail : "ambiguous");
    },
  });
  if (view === "loading")
    return (
      <AuthShell title="">
        <div role="status" aria-label="Checking your session">
          <Skeleton className="h-6 w-40" />
        </div>
      </AuthShell>
    );
  const heading = verificationCopy[view].heading;
  const body =
    view === "ready"
      ? `Confirm ${status.data?.email ?? ""} as the email address for your account.`
      : verificationCopy[view].body;
  return (
    <AuthShell
      title={
        <h1 ref={title} tabIndex={-1}>
          {heading}
        </h1>
      }
      description={body}
    >
      <div className="flex flex-col gap-4">
        <VerificationActions
          view={view}
          error={error}
          pending={confirmation.isPending}
          onVerify={() => {
            confirmation.mutate();
          }}
        />
      </div>
    </AuthShell>
  );
}
