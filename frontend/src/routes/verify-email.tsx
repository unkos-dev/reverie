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
  const view = verified
    ? "verified"
    : !usable || invalid || status.data?.email === null
      ? "invalid"
      : unauthenticated
        ? "signed-out"
        : status.data === undefined
          ? "loading"
          : "ready";
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
  const heading =
    view === "verified"
      ? "Email address verified"
      : view === "invalid"
        ? "This link can't be used"
        : view === "signed-out"
          ? "Sign in to verify your email"
          : "Verify your email address";
  const body =
    view === "verified"
      ? "Your email address is verified."
      : view === "invalid"
        ? "It may have expired, already been used, or belong to a different account. Send a new verification email from your email settings."
        : view === "signed-out"
          ? "Sign in to the account this email was sent to, then open the link in the email again."
          : `Confirm ${status.data?.email ?? ""} as the email address for your account.`;
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
        {view === "signed-out" ? (
          <Link to="/login" className="hover:text-fg underline underline-offset-4">
            Go to sign in
          </Link>
        ) : view !== "ready" ? (
          <Link to="/account/email" className="hover:text-fg underline underline-offset-4">
            Go to email settings
          </Link>
        ) : (
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
            <Button
              className="w-full"
              isDisabled={confirmation.isPending}
              onPress={() => {
                confirmation.mutate();
              }}
            >
              Verify email
            </Button>
          </>
        )}
      </div>
    </AuthShell>
  );
}
