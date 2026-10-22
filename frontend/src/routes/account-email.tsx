import { useMutation, useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState, type ReactElement } from "react";
import { Link } from "react-router";

import { ApiError } from "@/api";
import { fetchEmailVerification, requestEmailVerification } from "@/api/auth";
import { Badge } from "@/components/ui/badge";
import { Button } from "@/components/ui/button";
import { FieldError } from "@/components/ui/field";
import { Skeleton } from "@/components/ui/skeleton";
import { queryKeys } from "@/lib/query/keys";
import { AuthShell } from "./auth-shell";

export function Component(): ReactElement {
  const status = useQuery({
    queryKey: queryKeys.auth.emailVerification(),
    queryFn: ({ signal }) => fetchEmailVerification(signal),
    retry: false,
  });
  const [sent, setSent] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const title = useRef<HTMLHeadingElement>(null);
  const view =
    status.data === undefined
      ? "loading"
      : status.data.email === null
        ? "no-address"
        : status.data.email_verified
          ? "verified"
          : !status.data.mail_enabled
            ? "unavailable"
            : sent
              ? "sent"
              : "unverified";
  useEffect(() => {
    if (view !== "loading") title.current?.focus();
  }, [view]);
  const request = useMutation({
    mutationFn: () => requestEmailVerification(),
    onSuccess: () => {
      setSent(true);
      setError(null);
    },
    onError: (cause) => {
      setSent(false);
      setError(
        cause instanceof ApiError && cause.status === 503
          ? "Email isn't available right now. Try again later."
          : cause instanceof ApiError && cause.status === 429
            ? cause.detail
            : "Could not confirm the request. Try again.",
      );
    },
  });
  const footer = (
    <Link to="/library" className="hover:text-fg underline underline-offset-4">
      Back to library
    </Link>
  );
  if (status.data === undefined)
    return (
      <AuthShell title="" footer={footer}>
        {status.isError ? (
          <>
            <FieldError>Could not load your email settings.</FieldError>
            <Button
              className="mt-4 w-full"
              isDisabled={status.isFetching}
              onPress={() => {
                void status.refetch();
              }}
            >
              Try again
            </Button>
          </>
        ) : (
          <div role="status" aria-label="Loading email settings">
            <Skeleton className="h-6 w-40" />
          </div>
        )}
      </AuthShell>
    );
  const body =
    view === "no-address"
      ? "Your account has no email address. Ask your administrator to add one."
      : view === "verified"
        ? "Your email address is verified."
        : view === "unavailable"
          ? "Email isn't set up on this server, so this address can't be verified yet."
          : view === "sent"
            ? "Verification email requested. Open the link in it while signed in to this account. The email says when the link expires."
            : "Verify that you control this email address.";
  return (
    <AuthShell
      title={
        <h1 ref={title} tabIndex={-1}>
          Email address
        </h1>
      }
      description={body}
      footer={footer}
    >
      <div className="flex flex-col gap-4">
        {status.data.email !== null ? (
          <div className="flex flex-wrap items-center gap-2">
            <span className="break-all">{status.data.email}</span>
            <Badge variant="outline">
              {status.data.email_verified ? "Verified" : "Not verified"}
            </Badge>
          </div>
        ) : null}
        {error !== null ? <FieldError>{error}</FieldError> : null}
        {view === "sent" || view === "unverified" ? (
          <Button
            className="w-full"
            variant={sent ? "outline" : "default"}
            isDisabled={request.isPending}
            onPress={() => {
              request.mutate();
            }}
          >
            {sent ? "Send again" : "Send verification email"}
          </Button>
        ) : null}
      </div>
    </AuthShell>
  );
}
